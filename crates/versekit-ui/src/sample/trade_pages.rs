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

//! The trade pages on real data in a throwaway folder: what they show of
//! the stored UEX lists, that nothing of the sample page stands there, and
//! that nothing is cut off. Every test holds the language lock.

use egui::Pos2;
use versekit_core::language;

use super::SamplePage;
use super::docking::tests::{draw, draw_clicking};
use super::real_data::{folder, real_page};

pub(super) fn shows(texts: &[(String, Pos2)], part: &str) -> bool {
    texts.iter().any(|(text, _)| text.contains(part))
}

fn seconds() -> f64 {
    crate::trade::now()
}

/// A price list as 4.12 stores it: two terminals, three goods.
fn selling_file() -> String {
    let (fetched, date) = (seconds() - 3_600.0, (seconds() - 3.0 * 86_400.0) as i64);
    format!(
        r#"{{"terminals":{{"1":{{"o":"Port Olisar","s":"Stanton","q":0,"n":"Admin Olisar","t":"commodity"}},"2":{{"o":"Lorville","s":"Stanton","q":1,"n":"TDD Lorville","t":"commodity"}}}},"waren":{{"Gold":[{{"t":"1","n":"Admin Olisar","p":33000.0,"d":{date},"k":"1,2","z":1}},{{"t":"2","n":"TDD Lorville","p":31000.0,"d":{date},"k":"","z":5}}],"Copper":[{{"t":"1","n":"Admin Olisar","p":4400.0,"d":{date},"k":"","z":6}}],"Luminalia Gift":[{{"t":"1","n":"Admin Olisar","p":9000000.0,"d":{date},"k":"","z":1}}]}},"kaeufe":{{"Gold":[{{"t":"2","n":"TDD Lorville","p":2400.0,"d":{date},"k":"1,2,4","z":2,"m":50}}],"Iron":[{{"t":"1","n":"Admin Olisar","p":300.0,"d":{date},"k":"","z":7,"m":900}}],"Luminalia Gift":[{{"t":"1","n":"Admin Olisar","p":100.0,"d":{date},"k":"","z":7,"m":9}}]}},"format":5,"geholt":{fetched}}}"#
    )
}

fn stock_file() -> &'static str {
    r#"{"format": 1, "posten": [{"ware": "Gold", "menge": 10.0, "ort": "Levski", "gestohlen": false, "einkauf": 30000}]}"#
}

pub(super) fn page_with(name: &str, files: &[(&str, String)]) -> (SamplePage, std::path::PathBuf) {
    let dir = folder(name);
    for (file, text) in files {
        std::fs::write(dir.join(file), text).unwrap();
    }
    theme_and_language();
    let (page, _) = real_page_in(&dir);
    (page, dir)
}

fn theme_and_language() {
    versekit_core::theme::set_current("original");
    language::set_thread_language(language::Choice::De);
}

fn real_page_in(dir: &std::path::Path) -> (SamplePage, ()) {
    use versekit_core::paths::Paths;
    let host = super::real_data::host_in(dir, &[]);
    (SamplePage::with_settings(Paths::new(host)), ())
}

impl SamplePage {
    fn selling_take_from_stock(&mut self) {
        self.selling.take_from_stock();
    }

    fn selling_buy(&mut self, good: &str) {
        self.selling.buy(good);
    }

    fn selling_buy_overview(&mut self) {
        self.selling.set_buying(true);
    }

    fn selling_sell_mode(&mut self) {
        self.selling.set_buying(false);
    }
}

fn look(page: &mut SamplePage, key: &str, width: f32) -> (f32, Vec<(String, Pos2)>) {
    page.go_to(key);
    draw(page, width, 820.0)
}

#[test]
fn selling_without_prices_says_so_and_shows_nothing_made_up() {
    let _language = crate::language_lock();
    let (mut page, dir) = real_page("verkauf-leer", "", &[]);
    let (overflow, texts) = look(&mut page, "hf_verkauf", 1280.0);
    assert!(shows(&texts, &language::t("s_vk_kein_stand")), "{texts:?}");
    assert!(!shows(&texts, &language::t("s_mu_seite")), "no placeholder page");
    assert!(!shows(&texts, "Hier steht die Seite."));
    assert_eq!(overflow, 0.0);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn selling_lists_what_pays_best_and_the_places_of_a_choice() {
    let _language = crate::language_lock();
    let (mut page, dir) = page_with(
        "verkauf-daten",
        &[("verkauf.json", selling_file()), ("handelslager.json", stock_file().to_string())],
    );
    let (overflow, texts) = look(&mut page, "hf_verkauf", 1280.0);
    assert_eq!(overflow, 0.0);
    // The overview: the best prices, clickable.
    assert!(shows(&texts, "33.000 aUEC"), "{texts:?}");
    assert!(shows(&texts, "Gold"), "{texts:?}");
    assert!(shows(&texts, &language::t("s_hd_spitze")), "{texts:?}");
    // Gifts of an event are no goods to trade: they stay out of the top list although they pay most.
    assert!(!shows(&texts, "Luminalia"), "{texts:?}");
    assert!(!shows(&texts, "9.000.000"), "{texts:?}");
    page.selling_buy_overview();
    let (_, texts) = look(&mut page, "hf_verkauf", 1280.0);
    assert!(shows(&texts, &language::t("s_hd_k_spitze")), "{texts:?}");
    assert!(shows(&texts, "Iron") && !shows(&texts, "Luminalia"), "the cheap gift stays out: {texts:?}");
    page.selling_sell_mode();
    // Mein Lager takes Gold with its amount; the places show proceeds and profit.
    page.selling_take_from_stock();
    let (overflow, texts) = look(&mut page, "hf_verkauf", 1280.0);
    assert_eq!(overflow, 0.0);
    assert!(shows(&texts, "Admin Olisar") && shows(&texts, "TDD Lorville"), "{texts:?}");
    assert!(shows(&texts, "Ladung hier: 330.000"), "{texts:?}");
    assert!(shows(&texts, "Gewinn hier: +30.000"), "{texts:?}");
    assert!(shows(&texts, &language::t("s_vk_fuellt")), "the second place fills up");
    // Counter-check: the stock of the sample page is not there.
    assert!(!shows(&texts, "Titanium"));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn buying_lists_the_places_cheapest_first_with_stock_and_boxes() {
    let _language = crate::language_lock();
    let (mut page, dir) = page_with("kaufen-daten", &[("verkauf.json", selling_file())]);
    page.selling_buy("Gold");
    let (overflow, texts) = look(&mut page, "hf_verkauf", 1280.0);
    assert_eq!(overflow, 0.0);
    assert!(shows(&texts, "TDD Lorville"), "{texts:?}");
    assert!(shows(&texts, "2.400 aUEC je SCU"), "{texts:?}");
    assert!(shows(&texts, &language::t("s_vk_k_vorrat_2")), "{texts:?}");
    assert!(shows(&texts, "Kisten: 1, 2, 4 SCU"), "{texts:?}");
    assert!(shows(&texts, &language::t("s_vk_k_uebersicht")));
    let _ = std::fs::remove_dir_all(dir);
}

/// The trips from two terminals: there and back.
fn routes_file() -> String {
    let fetched = seconds() - 7_200.0;
    format!(
        r#"{{"starts":{{"1":{{"geholt":{fetched},"fahrten":[{{"ware":"Gold","ziel":"2","zielname":"TDD Lorville","zielort":"Hurston","zielsystem":"Stanton","ek":100.0,"vk":300.0,"gewinn_scu":200.0,"strecke":2.0,"vorrat":50,"bedarf":0}}]}},"2":{{"geholt":{fetched},"fahrten":[{{"ware":"Copper","ziel":"1","zielname":"Admin Olisar","zielort":"Crusader","zielsystem":"Stanton","ek":10.0,"vk":60.0,"gewinn_scu":50.0,"strecke":2.5,"vorrat":500,"bedarf":0}}]}}}},"format":1,"geholt":{fetched}}}"#
    )
}

pub(super) fn ships_file() -> String {
    let fetched = seconds() - 7_200.0;
    format!(
        r#"{{"schiffe":{{"1":{{"name":"MISC Freelancer MAX","scu":122,"werft":"MISC"}},"2":{{"name":"Drake Caterpillar","scu":576,"werft":"Drake"}}}},"kauf":{{"1":[{{"stelle":"Dealer","ort":"Lorville","system":"Stanton","preis":1500000.0}}]}},"miete":{{}},"format":5,"geholt":{fetched}}}"#
    )
}

impl SamplePage {
    fn routes_start(&mut self, ident: &str, name: &str) {
        self.routes.choose_start(&egui::Context::default(), ident, name);
    }
}

/// Draws until the text is there: the chains run beside the page.
fn wait_for(page: &mut SamplePage, key: &str, part: &str) -> (f32, Vec<(String, Pos2)>) {
    let mut last = look(page, key, 1280.0);
    for _ in 0..80 {
        if shows(&last.1, part) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
        last = look(page, key, 1280.0);
    }
    last
}

#[test]
fn routes_show_the_trips_from_the_start_and_the_chain_back() {
    let _language = crate::language_lock();
    let files = [("verkauf.json", selling_file()), ("routen.json", routes_file()), ("schiffe.json", ships_file())];
    let (mut page, dir) = page_with("routen-daten", &files);
    let (overflow, texts) = look(&mut page, "hf_routen", 1280.0);
    assert_eq!(overflow, 0.0);
    assert!(shows(&texts, &language::t("s_hd_rt_start_zuerst")), "{texts:?}");
    assert!(!shows(&texts, "Laranite"), "no sample ranking");
    page.routes_start("1", "Admin Olisar");
    page.routes.set_round(true);
    let (overflow, texts) = wait_for(&mut page, "hf_routen", "Gewinn 16.000");
    assert_eq!(overflow, 0.0);
    // 120 SCU hold, 50 SCU in stock: 50 * 200 = 10.000 on the single trip.
    assert!(shows(&texts, "10.000 aUEC"), "{texts:?}");
    assert!(shows(&texts, "verkaufen in TDD Lorville"), "{texts:?}");
    assert!(shows(&texts, &language::t("s_rt_grenze_vorrat")), "{texts:?}");
    // The chain of two trips: 10.000 there, 120 SCU of 500 in stock back (hold 120): 6.000.
    assert!(shows(&texts, "Gewinn 16.000"), "{texts:?}");
    // The ship fills the hold and says where it is for sale.
    page.routes.pick_ship("MISC Freelancer MAX");
    let (_, texts) = wait_for(&mut page, "hf_routen", "1.500.000 aUEC");
    assert!(shows(&texts, &format!("{}  1.500.000 aUEC", language::t("s_rt_kaufen"))), "{texts:?}");
    assert!(shows(&texts, "Dealer"), "{texts:?}");
    assert_eq!(page.routes.scu_text(), "122");
    let _ = std::fs::remove_dir_all(dir);
}

fn drawn(texts: &[(String, Pos2)], text: &str) -> usize {
    texts.iter().filter(|(shown, _)| shown == text).count()
}

/// The first text of the page that starts with `start`.
fn starting_with(texts: &[(String, Pos2)], start: &str) -> Option<String> {
    texts.iter().map(|(shown, _)| shown).find(|shown| shown.starts_with(start)).cloned()
}

/// Opens the field named `field` by a click, checks that the list stands open (the name
/// is drawn twice: on the field and as the first entry), clicks the entry that starts with
/// `entry` and checks that the field shows it now and the list is shut.
fn opens_and_takes(page: &mut SamplePage, field: &str, entry: &str) {
    let opened = draw_clicking(page, 1280.0, 820.0, &[field]).remove(0);
    assert_eq!(drawn(&opened, field), 2, "`{field}` opens a list: {opened:?}");
    let entry = starting_with(&opened, entry).unwrap_or_else(|| panic!("no entry `{entry}`: {opened:?}"));
    let taken = draw_clicking(page, 1280.0, 820.0, &[&entry]).remove(0);
    assert_eq!(drawn(&taken, field), 0, "the field shows the entry now: {taken:?}");
    assert_eq!(drawn(&taken, &entry), 1, "and the list is shut: {taken:?}");
}

#[test]
fn the_system_and_the_maker_field_of_the_routes_each_open_their_list_and_take_an_entry() {
    let _language = crate::language_lock();
    let files = [("verkauf.json", selling_file()), ("routen.json", routes_file()), ("schiffe.json", ships_file())];
    let (mut page, dir) = page_with("routen-felder", &files);
    let (systems, makers) = (language::t("s_rt_alle_systeme"), language::t("s_rt_alle_werften"));
    let texts = wait_for(&mut page, "hf_routen", &makers).1;
    assert!(shows(&texts, &systems) && shows(&texts, &makers), "both fields are there: {texts:?}");
    opens_and_takes(&mut page, &systems, "Stanton (");
    // The second field on the page: its own list, and the first one keeps its choice.
    opens_and_takes(&mut page, &makers, "Drake (");
    let texts = look(&mut page, "hf_routen", 1280.0).1;
    assert!(starting_with(&texts, "Stanton (").is_some() && !shows(&texts, &systems), "{texts:?}");
    assert!(starting_with(&texts, "Drake (").is_some() && !shows(&texts, &makers), "{texts:?}");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_field_of_the_shops_opens_its_list_and_takes_an_entry() {
    let _language = crate::language_lock();
    let files =
        [("laeden-katalog.json", catalog_file()), ("laeden.json", shops_file()), ("schiffe.json", ships_file())];
    let (mut page, dir) = page_with("shops-felder", &files);
    page.shops_pick("Arctic");
    let texts = look(&mut page, "hf_laeden", 1280.0).1;
    let all = ["s_ld_alle_bereiche", "s_ld_alle_arten", "s_ld_alle_groessen", "s_ld_alle_klassen", "s_ld_alle_gueten"]
        .map(language::t);
    let field = all.iter().find(|text| shows(&texts, text)).unwrap_or_else(|| panic!("no field: {texts:?}"));
    let opened = draw_clicking(&mut page, 1280.0, 820.0, &[field]).remove(0);
    assert_eq!(drawn(&opened, field), 2, "`{field}` opens a list: {opened:?}");
    let _ = std::fs::remove_dir_all(dir);
}

/// A catalog with three parts, the three shops that sell them and one price list of one part.
pub(super) fn catalog_file() -> String {
    let fetched = seconds() - 7_200.0;
    let format = versekit_core::shops::FORMAT_CATALOG;
    format!(
        r#"{{"namen":{{"arctic":101,"frost":102}},"kategorien":[["Systems","Coolers"],["Armor","Helmets"]],"teile":[{{"n":"Arctic","s":"u-101","k":0,"h":"ACME","g":"2","c":"Military","q":"A","o":["Lorville","Port Olisar"],"p":[1200.0,"Lorville"],"l":[[0,1200.0],[1,1500.0]]}},{{"n":"Frost","s":"id:102","k":0,"h":"","g":"1","c":"","q":"B","o":["Port Olisar"],"l":[[1,300.0]]}},{{"n":"Iron Helmet","s":"u-201","k":1,"h":"Armorsmith","g":"","c":"","q":"","o":["Lorville"],"p":[800,"Lorville"],"l":[[2,800.0]]}}],"preise_namen":{{"arctic":[1200.0,"Lorville"]}},"kennungen":["u-101","u-201"],"mit_laeden":true,"laeden":[["Shop A","Lorville","Stanton"],["Shop B","Port Olisar","Stanton"],["Tammany","Lorville","Stanton"]],"format":{format},"geholt":{fetched}}}"#
    )
}

pub(super) fn shops_file() -> String {
    let fetched = seconds() - 7_200.0;
    format!(
        r#"{{"teile":{{"u-101":{{"geholt":{fetched},"zeilen":[{{"laden":"Shop A","ort":"Lorville","system":"Stanton","preis":1200.0,"zustand":63}},{{"laden":"Shop B","ort":"Port Olisar","system":"Stanton","preis":1500.0,"zustand":100}}]}}}},"format":1,"geholt":{fetched}}}"#
    )
}

impl SamplePage {
    fn shops_pick(&mut self, name: &str) {
        self.shops.pick_by_name(name);
    }

    fn shops_place(&mut self, place: &str) {
        self.shops.want_place(place);
    }

    fn shops_type_place(&mut self, text: &str) {
        self.shops.type_place(text);
    }
}

/// Wunsch von Blackd0g84: a place shows its shops, a click on a shop shows all it sells with its price there, a part
/// of it opens as in the list and the way back leads to the shop; a shop is found by its name in the field too.
#[test]
fn a_place_lists_its_shops_and_a_shop_shows_its_whole_offer() {
    let _language = crate::language_lock();
    let files =
        [("laeden-katalog.json", catalog_file()), ("laeden.json", shops_file()), ("schiffe.json", ships_file())];
    let (mut page, dir) = page_with("shops-orte", &files);
    page.shops_place("Lorville");
    let (overflow, texts) = look(&mut page, "hf_laeden", 1280.0);
    assert_eq!(overflow, 0.0);
    let heading = language::tf("s_ld_shops_hier", &[("0", language::Arg::from("Lorville")), ("1", language::Arg::from("2"))]);
    assert!(shows(&texts, &heading), "{texts:?}");
    let row_a = format!("Shop A ({})", language::tf("s_ld_shop_teile", &[("0", language::Arg::from("1"))]));
    let row_t = format!("Tammany ({})", language::tf("s_ld_shop_teile", &[("0", language::Arg::from("1"))]));
    assert!(shows(&texts, &row_a) && shows(&texts, &row_t) && !shows(&texts, "Shop B ("), "only the shops of the place: {texts:?}");
    assert!(shows(&texts, "Arctic") && shows(&texts, "Iron Helmet"), "what is sold there stays below: {texts:?}");
    // A click on the shop opens what it sells, with its price there; nothing of another shop.
    draw_clicking(&mut page, 1280.0, 820.0, &[&row_a]);
    assert_eq!(page.shops.open_outlet_name().as_deref(), Some("Shop A"));
    let (overflow, texts) = look(&mut page, "hf_laeden", 1280.0);
    assert_eq!(overflow, 0.0);
    assert!(shows(&texts, "Shop A") && shows(&texts, "Lorville · Stanton"), "{texts:?}");
    assert!(starting_with(&texts, "Arctic").is_some() && texts.iter().any(|(t, _)| t.contains("1.200 aUEC")), "{texts:?}");
    assert!(!shows(&texts, "Iron Helmet") && !shows(&texts, "Frost"), "only what this shop sells: {texts:?}");
    // A part of the shop opens as in the list (the shops of the place, the shopping list); back leads to the shop.
    draw_clicking(&mut page, 1280.0, 820.0, &["Arctic"]);
    let texts = look(&mut page, "hf_laeden", 1280.0).1;
    assert!(shows(&texts, "Shop A") && shows(&texts, &language::t("s_ld_in_korb")), "the part with its shops: {texts:?}");
    let back_to = |name: &str| language::tf("s_ld_zurueck_zu", &[("0", language::Arg::from(name))]);
    draw_clicking(&mut page, 1280.0, 820.0, &[&back_to("Shop A")]);
    assert_eq!(page.shops.open_outlet_name().as_deref(), Some("Shop A"), "back to the shop, not to the list");
    draw_clicking(&mut page, 1280.0, 820.0, &[&back_to("Lorville")]);
    assert_eq!(page.shops.open_outlet_name(), None, "and from the shop back to the place");
    // The name of a shop in the field opens it at its place, without regard to case.
    page.shops_type_place("tammany");
    assert_eq!(page.shops.open_outlet_name().as_deref(), Some("Tammany"));
    let texts = look(&mut page, "hf_laeden", 1280.0).1;
    assert!(shows(&texts, "Iron Helmet") && texts.iter().any(|(t, _)| t.contains("800 aUEC")), "{texts:?}");
    assert!(!shows(&texts, &language::t("s_ld_ort_unbekannt")), "a shop is no unknown place");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn shops_list_the_areas_then_the_shops_of_a_part_cheapest_first() {
    let _language = crate::language_lock();
    let files =
        [("laeden-katalog.json", catalog_file()), ("laeden.json", shops_file()), ("schiffe.json", ships_file())];
    let (mut page, dir) = page_with("shops-daten", &files);
    let (overflow, texts) = look(&mut page, "hf_laeden", 1280.0);
    assert_eq!(overflow, 0.0);
    assert!(shows(&texts, &language::t("s_hd_ld_bereiche")), "{texts:?}");
    assert!(shows(&texts, &format!("{} (2)", language::t("s_uk_systems"))), "{texts:?}");
    assert!(shows(&texts, &format!("{} (1)", language::t("s_ld_ber_schiffe"))), "the ships are an area: {texts:?}");
    assert!(!shows(&texts, &language::t("s_mu_seite")));
    page.shops_pick("Arctic");
    let (overflow, texts) = look(&mut page, "hf_laeden", 1280.0);
    assert_eq!(overflow, 0.0);
    assert!(shows(&texts, "1.200 aUEC") && shows(&texts, "1.500 aUEC"), "{texts:?}");
    assert!(shows(&texts, "Shop A") && shows(&texts, "Lorville · Stanton"), "{texts:?}");
    assert!(shows(&texts, "Zustand 63 %"), "used goods show the condition: {texts:?}");
    assert!(!shows(&texts, "Zustand 100 %"), "new goods do not");
    // The cheapest shop stands on top.
    let line_of = |texts: &[(String, Pos2)], part: &str| texts.iter().find(|(t, _)| t == part).map(|(_, at)| at.y);
    let (a, b) = (line_of(&texts, "Shop A"), line_of(&texts, "Shop B"));
    assert!(a.zip(b).is_some_and(|(a, b)| a < b), "Shop A is cheaper and stands above Shop B: {a:?} {b:?}");
    // A place with a shop shows only its shops.
    page.shops_place("Port Olisar");
    let (_, texts) = look(&mut page, "hf_laeden", 1280.0);
    assert!(shows(&texts, "Shop B") && !shows(&texts, "Shop A"), "{texts:?}");
    // A place without a shop of the part says so and shows all of them.
    page.shops_place("Area18");
    let (_, texts) = look(&mut page, "hf_laeden", 1280.0);
    assert!(shows(&texts, "Area18") && shows(&texts, "Shop A") && shows(&texts, "Shop B"), "{texts:?}");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn shops_without_a_catalog_do_not_pretend() {
    let _language = crate::language_lock();
    let (mut page, dir) = page_with("shops-leer", &[]);
    let (overflow, texts) = look(&mut page, "hf_laeden", 1280.0);
    assert_eq!(overflow, 0.0);
    assert!(!shows(&texts, &language::t("s_mu_seite")), "no placeholder page");
    assert!(!shows(&texts, "Titanium") && !shows(&texts, "Laranite"), "nothing made up");
    // It says what it does: the catalog is built, or it could not be fetched (the test asks no network).
    let failed = language::t("s_hd_ld_ohne_katalog");
    let texts = wait_for(&mut page, "hf_laeden", &failed).1;
    assert!(shows(&texts, &failed), "{texts:?}");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn routes_without_trips_say_what_to_do() {
    let _language = crate::language_lock();
    let (mut page, dir) = page_with("routen-leer", &[]);
    let (overflow, texts) = look(&mut page, "hf_routen", 1280.0);
    assert_eq!(overflow, 0.0);
    assert!(shows(&texts, &language::t("s_hd_rt_start_zuerst")), "{texts:?}");
    assert!(!shows(&texts, &language::t("s_mu_seite")));
    let _ = std::fs::remove_dir_all(dir);
}

const LONG_PLACE: &str = "Terminal of the Grand Commodity Exchange of the Outer Rim Trading Consortium";
const LONG_SHOP: &str = "Shop of the Extraordinarily Well Stocked Armor and Weapons Company";

/// The same lists with names far longer than any real one.
fn long(text: String) -> String {
    text.replace("Admin Olisar", LONG_PLACE).replace("TDD Lorville", LONG_PLACE).replace("Shop A", LONG_SHOP)
}

/// Every size, every scheme, both languages, the smallest window of the size:
/// the lists inside their scroll areas must not stick out either.
#[test]
fn the_lists_of_the_trade_pages_hold_long_names_at_every_size() {
    let _language = crate::language_lock();
    let files = [
        ("verkauf.json", long(selling_file())),
        ("handelslager.json", stock_file().to_string()),
        ("routen.json", long(routes_file())),
        ("schiffe.json", ships_file()),
        ("laeden-katalog.json", catalog_file()),
        ("laeden.json", long(shops_file())),
    ];
    let (mut page, dir) = page_with("handel-lang", &files);
    page.routes_start("1", LONG_PLACE);
    page.shops_pick("Arctic");
    page.selling_take_from_stock();
    let mut findings = Vec::new();
    for scheme in versekit_core::theme::schemes() {
        versekit_core::theme::set_thread_current(scheme.id);
        for points in versekit_core::font_size::POINTS_RANGE.0..=versekit_core::font_size::POINTS_RANGE.1 {
            page.size = points;
            let (width, height) = versekit_core::font_size::min_window(points);
            for english in [false, true] {
                language::set_thread_language(if english { language::Choice::En } else { language::Choice::De });
                for key in ["hf_verkauf", "hf_routen", "hf_laeden"] {
                    page.go_to(key);
                    let (overflow, _) = draw(&mut page, width, height);
                    if overflow > 0.5 {
                        findings.push(format!(
                            "{key} {} Größe {points} englisch {english}: {overflow:.0} zu breit",
                            scheme.id
                        ));
                    }
                }
            }
        }
    }
    versekit_core::theme::set_current("original");
    language::set_thread_language(language::Choice::De);
    let _ = std::fs::remove_dir_all(dir);
    assert!(findings.is_empty(), "abgeschnitten:\n{}", findings.join("\n"));
}

/// Draws the frames of one input: the page lives on in the same context.
fn click_at(page: &mut SamplePage, at: Pos2, width: f32, height: f32) -> Vec<(String, Pos2)> {
    let ctx = egui::Context::default();
    crate::install(&ctx);
    let mut texts = Vec::new();
    let button = |pressed| egui::Event::PointerButton {
        pos: at,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    let steps: [Vec<egui::Event>; 6] = [
        vec![],
        vec![],
        vec![egui::Event::PointerMoved(at)],
        vec![egui::Event::PointerMoved(at)],
        vec![button(true)],
        vec![button(false)],
    ];
    for events in steps.into_iter().chain([vec![], vec![]]) {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, egui::vec2(width, height))),
            events,
            ..Default::default()
        };
        texts.clear();
        let mut output = ctx.run_ui(input, |ui| {
            page.show(ui);
        });
        fn collect(shape: &egui::Shape, into: &mut Vec<(String, Pos2)>) {
            match shape {
                egui::Shape::Text(text) => into.push((text.galley.text().to_string(), text.pos)),
                egui::Shape::Vec(shapes) => shapes.iter().for_each(|shape| collect(shape, into)),
                _ => {}
            }
        }
        output.shapes.iter().for_each(|clipped| collect(&clipped.shape, &mut texts));
        output.textures_delta.clear();
    }
    texts
}

/// A click on the text of a row is a click on the row (labels in a row must not take it).
#[test]
fn a_click_on_the_text_of_a_row_chooses_it() {
    let _language = crate::language_lock();
    let (mut page, dir) = page_with("verkauf-klick", &[("verkauf.json", selling_file())]);
    let (_, texts) = look(&mut page, "hf_verkauf", 1280.0);
    let gold = texts.iter().find(|(text, _)| text == "Gold").map(|(_, at)| *at).expect("Gold stands in the top list");
    // In the middle of the word: the click lands on the label, not on the free part of the row.
    let texts = click_at(&mut page, gold + egui::vec2(6.0, 6.0), 1280.0, 820.0);
    assert!(shows(&texts, "TDD Lorville"), "the click chose Gold and its places show: {texts:?}");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn the_trade_pages_are_not_cut_off_at_the_smallest_window() {
    let _language = crate::language_lock();
    let files = [
        ("verkauf.json", selling_file()),
        ("handelslager.json", stock_file().to_string()),
        ("routen.json", routes_file()),
        ("schiffe.json", ships_file()),
        ("laeden-katalog.json", catalog_file()),
        ("laeden.json", shops_file()),
    ];
    let (mut page, dir) = page_with("handel-rand", &files);
    page.routes_start("1", "Admin Olisar");
    page.shops_pick("Arctic");
    page.selling_take_from_stock();
    for width in [880.0, 1100.0, 1600.0] {
        for english in [false, true] {
            language::set_thread_language(if english { language::Choice::En } else { language::Choice::De });
            let (overflow, _) = look(&mut page, "hf_verkauf", width);
            assert_eq!(overflow, 0.0, "verkauf width {width}, english {english}");
            let (overflow, _) = look(&mut page, "hf_routen", width);
            assert_eq!(overflow, 0.0, "routen width {width}, english {english}");
            let (overflow, _) = look(&mut page, "hf_laeden", width);
            assert_eq!(overflow, 0.0, "shops width {width}, english {english}");
        }
    }
    language::set_thread_language(language::Choice::De);
    let _ = std::fs::remove_dir_all(dir);
}
