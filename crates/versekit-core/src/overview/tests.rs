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

//! The settings of the start page, read and written as 4.12 does.

use super::*;
use crate::paths::Host;
use crate::pypath::Flavor;

fn paths(name: &str, settings: &str) -> (Paths, std::path::PathBuf) {
    let root = std::env::temp_dir().join(format!("versekit-overview-{name}-{}", crate::testdir::run_tag()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let data = root.to_string_lossy().into_owned();
    if !settings.is_empty() {
        std::fs::write(root.join("einstellungen.json"), settings).unwrap();
    }
    let host = Host::new(Flavor::Posix, &[("HOME", data.as_str()), ("SC_BP_HOME", data.as_str())], &data);
    (Paths::new(host), root)
}

fn ids(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| s.to_string()).collect()
}

#[test]
fn from_the_factory_all_tiles_show_but_the_play_time_while_the_head_bar_hides_it() {
    let (p, root) = paths("factory", "");
    let visible = visible_tiles(&p);
    assert_eq!(visible.len(), 16);
    assert!(!visible.iter().any(|id| id == "spielzeit"));
    assert_eq!(hidden_tiles(&p), ids(&["spielzeit"]));
    p.set_setting(PLAYTIME_HEAD_SETTING, Value::Bool(true));
    assert_eq!(visible_tiles(&p).len(), 17, "with the play time in the head bar it shows too");
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn hiding_writes_every_hidden_tile_and_then_the_head_bar_no_longer_counts() {
    let (p, root) = paths("hide", "");
    assert!(set_visible(&p, "rechner", false));
    assert_eq!(list(&p.settings(), HIDDEN_SETTING), Some(ids(&["spielzeit", "rechner"])));
    p.set_setting(PLAYTIME_HEAD_SETTING, Value::Bool(true));
    assert!(!visible_tiles(&p).iter().any(|id| id == "spielzeit"), "the list holds it now");
    set_visible(&p, "spielzeit", true);
    set_visible(&p, "rechner", true);
    assert_eq!(list(&p.settings(), HIDDEN_SETTING), Some(Vec::new()));
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn the_order_drops_unknown_ids_and_adds_new_ones_at_the_end() {
    let (p, root) = paths("order", r#"{"uebersicht_reihenfolge": ["lager", "weg", "spiel"]}"#);
    let order = tile_order(&p);
    assert_eq!(&order[..3], &ids(&["lager", "spiel", "standort"])[..]);
    assert_eq!(order.len(), 17);
    let swapped = swap(&p, "lager", "sicherung");
    assert_eq!(swapped[0], "sicherung");
    assert_eq!(swapped[14], "lager");
    assert_eq!(list(&p.settings(), ORDER_SETTING), Some(swapped), "the whole order is written");
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn a_tile_not_shown_here_keeps_its_place_in_the_order() {
    let stored = ids(&["spiel", "raffinerie", "lager", "bedarf", "funde"]);
    let shown = ids(&["funde", "lager", "spiel"]);
    assert_eq!(merged_order(&stored, &shown), ids(&["funde", "raffinerie", "lager", "bedarf", "spiel"]));
}

#[test]
fn which_area_comes_first() {
    let (p, root) = paths("first", r#"{"startseite_zuerst": "irgendwas"}"#);
    assert!(!cards_first(&p), "any other value means the tiles");
    p.set_setting(FIRST_SETTING, first_value(true));
    assert!(cards_first(&p));
    assert_eq!(p.setting(FIRST_SETTING).as_deref(), Some("karten"));
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn pinned_pages_are_known_ones_once_and_old_names_count_as_the_new_page() {
    let (p, root) =
        paths("pinned", r#"{"karten_angeheftet": ["hangar", "lager", "meinlager", "gibtsnicht", "laeden"]}"#);
    assert_eq!(pinned(&p), ids(&["hangar", "meinlager", "laeden"]));
    assert_eq!(set_pinned(&p, "routen", true), ids(&["hangar", "meinlager", "laeden", "routen"]));
    assert_eq!(swap_pinned(&p, "hangar", "routen"), ids(&["routen", "meinlager", "laeden", "hangar"]));
    assert_eq!(set_pinned(&p, "meinlager", false), ids(&["routen", "laeden", "hangar"]));
    assert_eq!(list(&p.settings(), PINNED_SETTING), Some(ids(&["routen", "laeden", "hangar"])));
    let _ = std::fs::remove_dir_all(root);
    // The old names alone: both lead to my storage, once.
    let (p, root) = paths("pinned-old", r#"{"karten_angeheftet": ["handelslager", "lager"]}"#);
    assert_eq!(pinned(&p), ids(&["meinlager"]));
    let _ = std::fs::remove_dir_all(root);
    // The dismantle calculation is a view of the wreck page: both old cards are the one page, where the
    // first of them stood.
    let (p, root) = paths("pinned-bergen", r#"{"karten_angeheftet": ["hangar", "zerlegen", "bergung"]}"#);
    assert_eq!(pinned(&p), ids(&["hangar", "bergung"]));
    assert_eq!(set_pinned(&p, "laeden", true), ids(&["hangar", "bergung", "laeden"]));
    assert_eq!(list(&p.settings(), PINNED_SETTING), Some(ids(&["hangar", "bergung", "laeden"])));
    let _ = std::fs::remove_dir_all(root);
    let (p, root) = paths("pinned-bergen-alone", r#"{"karten_angeheftet": ["zerlegen", "hangar"]}"#);
    assert_eq!(pinned(&p), ids(&["bergung", "hangar"]), "the old card is not lost");
    let _ = std::fs::remove_dir_all(root);
    let (p, root) = paths("pinned-bergen-key", "");
    assert_eq!(page_key("zerlegen"), None, "the page has no name of its own any more");
    assert!(pinned(&p).is_empty());
    assert_eq!(page_key("bergung"), Some("hf_bergung"));
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn the_old_cargo_hold_opens_my_storage_on_the_goods_for_sale_and_the_old_material_storage_without_a_choice() {
    let sale = Preset { purpose: "sale", view: "material" };
    assert_eq!(resolve("handelslager"), ("meinlager", Some(sale)));
    assert_eq!(resolve("lager"), ("meinlager", None));
    assert_eq!(resolve("meinlager"), ("meinlager", None), "the page itself brings no choice");
    assert_eq!(resolve("hangar"), ("hangar", None));
    assert_eq!(resolve("wunschliste"), ("hangar", None));
    assert_eq!(page_key(resolve("handelslager").0), Some("hf_meinlager"));
}

#[test]
fn a_card_of_the_wishlist_counts_as_the_hangar_once() {
    // The wishlist is the section "Planned" of the hangar page: its card is the card of the hangar.
    let (p, root) = paths("pinned-wunsch", r#"{"karten_angeheftet": ["wunschliste", "laeden"]}"#);
    assert_eq!(pinned(&p), ids(&["hangar", "laeden"]));
    let _ = std::fs::remove_dir_all(root);
    // Pinned too: one card, at the place of the first.
    let (p, root) = paths("pinned-wunsch-doppelt", r#"{"karten_angeheftet": ["hangar", "laeden", "wunschliste"]}"#);
    assert_eq!(pinned(&p), ids(&["hangar", "laeden"]));
    let _ = std::fs::remove_dir_all(root);
    // The page of the wishlist has no key of its own any more; the hangar keeps its own.
    assert_eq!(page_key("wunschliste"), None);
    assert_eq!(page_key("hangar"), Some("hf_hangar"));
}

#[test]
fn a_card_of_the_page_to_name_the_ships_counts_as_the_hangar() {
    let (p, root) = paths("pinned-asop", r#"{"karten_angeheftet": ["asop", "laeden", "hangar"]}"#);
    assert_eq!(pinned(&p), ids(&["hangar", "laeden"]));
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn the_pinned_evaluation_page_counts_as_the_overview_of_the_statistics() {
    let (p, root) =
        paths("pinned-statistik", r#"{"karten_angeheftet": ["statistik_auswertung", "hangar", "statistik"]}"#);
    assert_eq!(pinned(&p), ids(&["statistik", "hangar"]), "once, at the place of the first");
    let _ = std::fs::remove_dir_all(root);
    let (p, root) = paths("pinned-auswertung", r#"{"karten_angeheftet": ["statistik_auswertung"]}"#);
    assert_eq!(pinned(&p), ids(&["statistik"]));
    let _ = std::fs::remove_dir_all(root);
    // The page itself is gone from the list of pages that can be pinned.
    assert_eq!(page_key("statistik_auswertung"), None);
    assert_eq!(page_key("statistik"), Some("hf_st_uebersicht"));
}

/// The shader cache is the last card of the graphics page since 5.1.4: a pinned `shader` is the page Grafik, once.
#[test]
fn the_pinned_shader_cache_counts_as_the_graphics_page() {
    let (p, root) = paths("pinned-shader", r#"{"karten_angeheftet": ["shader", "hangar", "grafik"]}"#);
    assert_eq!(pinned(&p), ids(&["grafik", "hangar"]), "once, at the place of the first");
    let _ = std::fs::remove_dir_all(root);
    assert_eq!(resolve("shader").0, "grafik");
    assert_eq!(page_key("shader"), None, "no page of its own any more");
    assert_eq!(page_key(resolve("shader").0), Some("hf_grafik"));
}

#[test]
fn every_page_and_tile_target_has_its_key() {
    assert_eq!(page_key("laeden"), Some("hf_laeden"));
    assert_eq!(page_name("hf_st_uebersicht"), Some("statistik"));
    for (tile, _) in TILES {
        assert!(tile_target(tile).is_some(), "{tile}");
        assert!(tile_title(tile).is_some(), "{tile}");
    }
    assert_eq!(description_key("hangar"), "k_hangar");
    assert_eq!(tile_target("notizen"), Some("hf_notizen"));
    assert_eq!(page_name("hf_notizen"), Some("notizen"));
    assert_eq!(tile_target("executive_hangar"), Some("hf_contested"));
    assert_eq!(tile_title("executive_hangar"), Some("s_cz_hangar"));
}

fn entry(name: &str, zeit: &str, quelle: &str) -> Value {
    Value::Object(vec![
        ("name".into(), Value::str(name)),
        ("zeit".into(), Value::str(zeit)),
        ("quelle".into(), Value::str(quelle)),
    ])
}

#[test]
fn blueprint_numbers_count_owned_of_the_catalog_and_the_week_over_every_entry() {
    let now = crate::export::local_stamp("2026-10-22 20:00:00").unwrap();
    let own = vec![
        ("a".to_string(), entry("Arclight", "2026-10-20 10:00:00", "spiel")),
        ("b".to_string(), entry("Behring", "2026-10-01 10:00:00", "spiel")),
        ("c".to_string(), entry("", "2026-10-22 19:00:00", "basetool")),
        ("d".to_string(), entry("Dust", "kaputt", "spiel")),
    ];
    let numbers = blueprint_numbers(&own, &|key| key != "c", 10, now);
    assert_eq!((numbers.mine, numbers.total, numbers.week), (3, 10, 2));
    assert_eq!(numbers.newest, "c", "a nameless entry is named by its key");
    assert_eq!(numbers.newest_at, crate::export::local_stamp("2026-10-22 19:00:00"));
    let finds = latest_finds(&own, 3);
    assert_eq!(finds.iter().map(|f| f.0.as_str()).collect::<Vec<_>>(), ["Arclight", "Behring"], "no basetool");
}

#[test]
fn play_time_of_the_week_cuts_what_lies_before() {
    let now = 1_000_000;
    let spans = [(now - WEEK_SEC - 100, now - WEEK_SEC + 50), (now - 60, now + 30)];
    assert_eq!(playtime_week(&spans, now), 50 + 60);
}

#[test]
fn open_contracts_count_each_title_once_with_the_most_missing_first() {
    let running = ids(&["Beta", "alpha", "Beta", "", "Gamma", "Ohne"]);
    let check = |name: &str| match name {
        "Beta" => Some((5, 1)),
        "alpha" => Some((3, 3)),
        "Gamma" => Some((2, 1)),
        _ => None,
    };
    let found = open_contracts(&running, &check);
    let names: Vec<&str> = found.iter().map(|f| f.0.as_str()).collect();
    assert_eq!(names, ["alpha", "Beta", "Gamma"]);
}

#[test]
fn the_watch_list_counts_names_owned_and_keeps_patterns_open() {
    let data =
        crate::jsonio::loads(r#"{"namen": ["Zeta", "Alpha", "Beta"], "eintraege": [{"titel": "Helm"}, {}]}"#).unwrap();
    let owned = |key: &str| key == crate::paths::name_key("Beta");
    let (found, total, open) = watch_numbers(&data, &owned);
    assert_eq!((found, total), (1, 5));
    assert_eq!(open, ids(&["Alpha", "Zeta", "Helm", "?"]));
}

#[test]
fn refinery_jobs_split_into_running_and_done_and_old_ones_go() {
    let job = |ends: f64| vec![("ends".to_string(), Value::Float(ends))];
    let now = 100_000.0;
    let (running, done) = refinery_current(&[job(now + 60.0), job(now - 10.0), job(now - KEEP_DONE_SEC - 1.0)], now);
    assert_eq!((running.len(), done.len()), (1, 1));
    assert_eq!(countdown_text(2.0 * 86_400.0 + 4.0 * 3600.0 + 5.0), "2d 4h");
    assert_eq!(countdown_text(3600.0 + 180.0), "1h 3m");
    assert_eq!(countdown_text(43.0 * 60.0 + 57.0), "43m 57s");
    assert_eq!(countdown_text(-3.0), "0s");
}

#[test]
fn the_overlay_lines_of_the_refinery_put_the_running_first_and_mark_the_done() {
    let job = |ends: f64, material: &str| {
        vec![
            ("ends".to_string(), Value::Float(ends)),
            ("materials".to_string(), Value::Array(vec![Value::str(material)])),
        ]
    };
    let now = 100_000.0;
    let jobs = [job(now - 10.0, "Gold"), job(now + 600.0, "Quarz"), job(now + 60.0, "Iron")];
    let lines = refinery_lines(&jobs, now);
    let marks: Vec<bool> = lines.iter().map(|(_, done)| *done).collect();
    assert_eq!(marks, [false, false, true], "{lines:?}");
    assert!(lines[0].0.starts_with("Iron") && lines[0].0.contains("1m 0s"), "the nearest end first: {lines:?}");
    assert!(lines[1].0.starts_with("Quarz") && lines[1].0.contains("10m 0s"), "{lines:?}");
    assert!(lines[2].0.starts_with("Gold") && !lines[2].0.contains("0s"), "a done one has no time left: {lines:?}");
    assert!(refinery_lines(&[], now).is_empty());
}

#[test]
fn storage_counts_names_and_puts_counted_materials_last() {
    use crate::stock::{Row, Store};
    let row = |store: Store, name: &str, amount: f64, place: &str, piece: bool, item: bool| Row {
        store,
        index: 0,
        name: name.into(),
        amount,
        piece,
        quality: None,
        place: place.into(),
        stolen: false,
        cost: None,
        item,
        leftover: false,
    };
    let rows = vec![
        row(Store::Raw, "Gold", 10.0, "Area18", false, false),
        row(Store::Raw, "gold", 30.0, "Lorville", false, false),
        row(Store::Raw, "Titanium", 20.0, "Area18", false, false),
        row(Store::Raw, "Saldynium", 99.0, "Orison", true, false),
        row(Store::Trade, "Agricium", 5.0, "", false, false),
        row(Store::Trade, "Helm", 1.0, "", true, true),
    ];
    assert_eq!(storage_counts(&rows), (3, 1, 1));
    let top = storage_top(&rows, 3);
    let names: Vec<(&str, f64, &str)> = top.iter().map(|t| (t.0.as_str(), t.1, t.2.as_str())).collect();
    assert_eq!(names, [("Gold", 40.0, "Lorville"), ("Titanium", 20.0, "Area18"), ("Saldynium", 99.0, "Orison")]);
}

#[test]
fn sizes_are_kept_by_id() {
    let (p, root) = paths("sizes", "");
    assert!(sizes(&p).is_empty());
    let kept = vec![("spielzeit".to_string(), "gross".to_string()), ("lager".to_string(), "breit".to_string())];
    p.set_setting(SIZE_SETTING, sizes_value(&kept));
    assert_eq!(sizes(&p), kept);
    let _ = std::fs::remove_dir_all(root);
}

fn stamp(text: &str) -> i64 {
    crate::export::local_stamp(text).expect("a readable time")
}

/// Local time text of a time stamp, as the collection writes it.
fn text_of(seconds: i64) -> String {
    let time = crate::formats::local_time(seconds).expect("a local time");
    crate::formats::strftime("%Y-%m-%d %H:%M:%S", &time)
}

#[test]
fn the_periods_begin_where_the_calendar_and_the_clock_say() {
    let now = stamp("2026-10-22 20:00:00");
    // Today and the month are the local calendar day and month; the first second belongs to them.
    assert_eq!(Period::Today.since(now), Some(stamp("2026-10-22 00:00:00")));
    assert!(Period::Today.contains(stamp("2026-10-22 00:00:00"), now));
    assert!(!Period::Today.contains(stamp("2026-10-21 23:59:59"), now));
    assert_eq!(Period::Month.since(now), Some(stamp("2026-10-01 00:00:00")));
    assert!(Period::Month.contains(stamp("2026-10-01 00:00:00"), now));
    assert!(!Period::Month.contains(stamp("2026-09-30 23:59:59"), now));
    // Seven and fourteen days roll with the clock, not with the calendar.
    assert!(Period::Days7.contains(now - WEEK_SEC, now));
    assert!(!Period::Days7.contains(now - WEEK_SEC - 1, now));
    assert!(Period::Days14.contains(now - 2 * WEEK_SEC, now));
    assert!(!Period::Days14.contains(now - 2 * WEEK_SEC - 1, now));
    assert!(Period::All.contains(0, now) && Period::All.since(now).is_none());
    // Just after midnight "today" is only those seconds, "7 days" still the whole week.
    let early = stamp("2026-10-22 00:00:10");
    assert!(!Period::Today.contains(stamp("2026-10-21 23:00:00"), early));
    assert!(Period::Days7.contains(stamp("2026-10-21 23:00:00"), early));
}

#[test]
fn today_and_the_month_follow_the_local_day_across_the_change_of_the_clock() {
    // Whatever the zone of the machine: the start is the local midnight of that day, also on the days the
    // clock is turned.
    for day in ["2026-03-29", "2026-10-25", "2026-01-01", "2026-12-31"] {
        let now = stamp(&format!("{day} 15:30:00"));
        assert_eq!(Period::Today.since(now), Some(stamp(&format!("{day} 00:00:00"))), "{day}");
        let month = &day[..8];
        assert_eq!(Period::Month.since(now), Some(stamp(&format!("{month}01 00:00:00"))), "{day}");
    }
}

#[test]
fn the_received_list_takes_every_entry_with_a_time_newest_first() {
    let own = vec![
        ("b".to_string(), entry("beta", "2026-10-20 10:00:00", "hand")),
        ("a".to_string(), entry("Alpha", "2026-10-20 10:00:00", "log")),
        ("c".to_string(), entry("", "2026-10-22 19:00:00", "basetool")),
        ("d".to_string(), entry("Dust", "kaputt", "log")),
        ("e".to_string(), Value::Object(vec![("name".into(), Value::str("Ohne Zeit"))])),
        ("f".to_string(), entry("Fern", "2026-10-01 10:00:00", "start")),
    ];
    let list = received(&own);
    let order: Vec<(&str, &str)> = list.iter().map(|r| (r.key.as_str(), r.source.as_str())).collect();
    assert_eq!(order, [("c", "basetool"), ("a", "log"), ("b", "hand"), ("f", "start")], "equal seconds by name");
    assert_eq!(list[0].name, "c", "a nameless entry is named by its key");
}

#[test]
fn the_list_counts_exactly_what_the_tile_calls_this_week() {
    let base = stamp("2026-10-22 20:00:00");
    let mut own: Vec<(String, Value)> = [-3 * 86_400, -WEEK_SEC, -WEEK_SEC - 1, -10 * 86_400, 0, 3_600, -1]
        .into_iter()
        .enumerate()
        .map(|(n, offset)| {
            let source = ["log", "hand", "basetool"][n % 3];
            (format!("k{n}"), entry(&format!("Name {n}"), &text_of(base + offset), source))
        })
        .collect();
    own.push(("none".to_string(), Value::Object(vec![("name".into(), Value::str("Ohne"))])));
    own.push(("broken".to_string(), entry("Kaputt", "gestern", "log")));
    let list = received(&own);
    // Around every edge of the week, also with the clock behind some entries (a time after now counts).
    for now in [base - 5 * 86_400, base - 1, base, base + 1, base + WEEK_SEC, base + WEEK_SEC + 1, base + 30 * 86_400] {
        let week = blueprint_numbers(&own, &|_| true, 0, now).week;
        assert_eq!(count_in(&list, Period::Days7, now), week, "now {now}");
    }
}
