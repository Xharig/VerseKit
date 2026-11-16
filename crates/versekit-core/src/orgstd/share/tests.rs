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

use super::*;
use crate::stock::Store;

fn row(store: Store, name: &str, amount: f64, place: &str) -> Row {
    Row {
        store,
        index: 0,
        name: name.into(),
        amount,
        piece: false,
        quality: None,
        place: place.into(),
        stolen: false,
        cost: None,
        item: false,
        leftover: false,
    }
}

fn raw(name: &str, amount: f64, quality: Option<i64>, place: &str) -> Row {
    Row { quality, ..row(Store::Raw, name, amount, place) }
}

fn goods(name: &str, amount: f64, place: &str) -> Row {
    row(Store::Trade, name, amount, place)
}

#[test]
fn only_names_the_program_knows_for_sure_go_out_and_the_rest_is_counted() {
    let data = StockData::sample();
    let rows = [
        raw("Agricium", 3.0, Some(700), "Area18"),
        goods("Laranite", 5.0, "Levski"),
        goods("Venture Arms", 2.0, "").tap(|r| {
            r.item = true;
            r.piece = true;
        }),
        raw("Agricium Erz (Eigenname)", 4.0, None, ""),
        goods("Geheimware", 1.0, ""),
    ];
    let share = stock_share(&rows, &data);
    let names: Vec<&str> = share.items.iter().map(|i| i.item.as_str()).collect();
    assert_eq!(names, ["Agricium", "Laranite", "Venture Arms"]);
    assert_eq!(share.left.unknown, 2);
    assert_eq!(share.refusal(), None);
}

trait Tap: Sized {
    fn tap(self, edit: impl FnOnce(&mut Self)) -> Self;
}

impl Tap for Row {
    fn tap(mut self, edit: impl FnOnce(&mut Self)) -> Self {
        edit(&mut self);
        self
    }
}

#[test]
fn the_names_go_out_as_the_lists_spell_them_not_as_typed() {
    let data = StockData::sample();
    let share = stock_share(&[raw("  agricium ", 1.0, None, "")], &data);
    assert_eq!(share.items[0].item, "Agricium");
}

#[test]
fn stolen_goods_and_empty_lines_stay_home_and_are_counted() {
    let data = StockData::sample();
    let rows = [
        goods("Laranite", 5.0, "Levski").tap(|r| r.stolen = true),
        goods("Laranite", 0.0, "Levski"),
        goods("Laranite", f64::NAN, "Levski"),
        goods("Laranite", 0.0004, "Levski"),
        goods("Laranite", 1.0, "Orison"),
    ];
    let share = stock_share(&rows, &data);
    assert_eq!((share.left.stolen, share.left.empty, share.items.len()), (1, 3, 1));
}

#[test]
fn neither_the_price_nor_any_other_field_reaches_a_line() {
    let data = StockData::sample();
    let priced = goods("Laranite", 5.0, "Levski").tap(|r| r.cost = Some(123_456.789));
    let share = stock_share(&[priced], &data);
    let sent = serde_json::to_string(&share.to_json()).unwrap();
    assert!(
        !sent.contains("123456")
            && !sent.contains("einkauf")
            && !sent.contains("stolen")
            && !sent.contains("gestohlen"),
        "{sent}"
    );
    let lines = share.to_json();
    let keys: Vec<&str> = lines[0].as_object().unwrap().keys().map(String::as_str).collect();
    assert!(keys.iter().all(|k| ["item", "quantity", "unit", "quality", "location"].contains(k)), "{keys:?}");
}

#[test]
fn equal_lines_are_summed_and_unlike_lines_stay_apart() {
    let data = StockData::sample();
    let rows = [
        raw("Agricium", 1.25, Some(500), "Area18"),
        raw("Agricium", 2.0, Some(500), "Area18"),
        raw("Agricium", 4.0, Some(600), "Area18"),
        raw("Agricium", 8.0, Some(500), "Levski"),
    ];
    let share = stock_share(&rows, &data);
    assert_eq!(share.items.len(), 3);
    assert_eq!(share.merged, 1);
    let first = share.items.iter().find(|i| i.quality == Some(500) && i.location == "Area18").unwrap();
    assert_eq!(first.quantity, 3.25);
}

#[test]
fn scu_keep_three_decimals_and_pieces_are_whole() {
    let data = StockData::sample();
    let share =
        stock_share(&[goods("Laranite", 1.23456, ""), raw("Hadanite", 2.4, None, "").tap(|r| r.piece = true)], &data);
    let by = |name: &str| share.items.iter().find(|i| i.item == name).unwrap();
    assert_eq!((by("Laranite").quantity, by("Laranite").unit), (1.235, Unit::Scu));
    assert_eq!((by("Hadanite").quantity, by("Hadanite").unit), (2.0, Unit::Piece));
}

#[test]
fn a_quality_outside_the_standard_is_left_off_and_a_long_place_is_cut_and_told() {
    let data = StockData::sample();
    let place = "o".repeat(300);
    let share = stock_share(&[raw("Agricium", 1.0, Some(1001), &place), raw("Iron", 1.0, Some(-5), "")], &data);
    assert_eq!(share.items.iter().map(|i| i.quality).collect::<Vec<_>>(), [None, None]);
    let long = share.items.iter().find(|i| !i.location.is_empty()).unwrap();
    assert_eq!(long.location.chars().count(), TEXT_MAX);
    assert_eq!(share.left.shortened, 1);
}

#[test]
fn a_list_over_the_limit_is_never_cut_and_never_sent() {
    let data = StockData::sample();
    let rows: Vec<Row> = (0..=STOCK_MAX).map(|n| goods("Laranite", 1.0, &format!("Ort {n}"))).collect();
    let share = stock_share(&rows, &data);
    assert_eq!(share.too_many, Some(STOCK_MAX + 1));
    assert_eq!(share.items.len(), STOCK_MAX + 1, "nothing is cut");
    assert_eq!(share.refusal().map(|e| e.code), Some(E_LIMIT));
    let exactly: Vec<Row> = (0..STOCK_MAX).map(|n| goods("Laranite", 1.0, &format!("Ort {n}"))).collect();
    assert_eq!(stock_share(&exactly, &data).refusal(), None);
}

#[test]
fn an_empty_list_is_refused_here_so_it_never_replaces_what_the_org_has() {
    let data = StockData::sample();
    assert_eq!(stock_share(&[], &data).refusal().map(|e| e.code), Some(E_EMPTY));
    assert_eq!(stock_share(&[goods("Geheimware", 1.0, "")], &data).refusal().map(|e| e.code), Some(E_EMPTY));
    assert_eq!(blueprint_share(Vec::new()).refusal().map(|e| e.code), Some(E_EMPTY));
}

#[test]
fn blueprints_go_out_once_each_in_the_spelling_of_the_list() {
    let share = blueprint_share(["P4-AR Rifle", "  Arclight Pistol ", "p4-ar rifle", "", "\u{7}"].map(String::from));
    assert_eq!(share.names, ["P4-AR Rifle", "Arclight Pistol"]);
    assert_eq!((share.twice, share.empty), (1, 2));
}

#[test]
fn more_blueprints_than_the_standard_takes_are_refused_not_cut() {
    let share = blueprint_share((0..=BLUEPRINTS_MAX).map(|n| format!("Bauplan {n}")));
    assert_eq!(share.too_many, Some(BLUEPRINTS_MAX + 1));
    assert_eq!(share.refusal().map(|e| e.code), Some(E_LIMIT));
    assert_eq!(blueprint_share((0..BLUEPRINTS_MAX).map(|n| format!("Bauplan {n}"))).refusal(), None);
}

#[test]
fn the_own_blueprints_go_out_by_the_names_of_the_catalog_without_the_starting_ones_and_the_unknown() {
    use crate::catalog_cache::{Blueprint, CatalogCache};
    let blueprint =
        |name: &str| Blueprint { key: crate::paths::name_key(name), name: name.into(), ..Blueprint::default() };
    let catalog = CatalogCache {
        blueprints: vec![blueprint("P4-AR Rifle"), blueprint("Arclight Pistol")],
        ..CatalogCache::default()
    };
    let entry = |name: &str, source: &str| (name.to_string(), source.to_string());
    let share = own_blueprint_share(
        [
            entry("p4-ar rifle", "log"),
            entry("Arclight Pistol", "start"),
            entry("Übersetzter Name", "hand"),
            entry("Arclight Pistol", "basetool"),
        ],
        &catalog,
    );
    assert_eq!(share.names, ["P4-AR Rifle", "Arclight Pistol"], "in the spelling of the catalog");
    assert_eq!((share.unknown, share.twice), (1, 0), "the starting blueprint is not counted at all");
    // Only starting blueprints: nothing to share, and the empty list is refused.
    let only_start = own_blueprint_share([entry("Arclight Pistol", "start")], &catalog);
    assert_eq!(only_start.refusal().map(|e| e.code), Some(E_EMPTY));
}

#[test]
fn a_key_has_128_bits_and_no_two_are_alike() {
    let keys: std::collections::HashSet<String> = (0..50).map(|_| idempotency_key()).collect();
    assert_eq!(keys.len(), 50);
    assert!(keys.iter().all(|k| k.len() == 32 && k.chars().all(|c| c.is_ascii_hexdigit())));
}
