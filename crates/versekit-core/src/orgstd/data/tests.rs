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

use serde_json::json;

use super::*;

#[test]
fn a_good_stock_row_is_taken_over_and_unknown_fields_are_passed_by() {
    let rows = [
        json!({ "item": "Quantanium", "quantity": 12.5, "unit": "scu", "quality": 650, "location": "Area18", "member": "Pilot", "price": 99 }),
    ];
    let parsed = parse_lots(&rows);
    assert_eq!(parsed.dropped, 0);
    assert_eq!(
        parsed.rows,
        [OrgLot {
            item: "Quantanium".into(),
            quantity: 12.5,
            unit: Unit::Scu,
            quality: Some(650),
            location: "Area18".into(),
            member: "Pilot".into()
        }]
    );
}

#[test]
fn a_broken_stock_row_falls_away_alone_and_is_counted() {
    let rows = [
        json!({ "item": "Gut", "quantity": 1 }),
        json!({ "item": "Negativ", "quantity": -1 }),
        json!({ "item": "Zu viel", "quantity": 1e12 }),
        json!({ "item": "Qualität zu hoch", "quantity": 1, "quality": 1001 }),
        json!({ "item": "Qualität unter 0", "quantity": 1, "quality": -1 }),
        json!({ "item": "Qualität Text", "quantity": 1, "quality": "hoch" }),
        json!({ "item": "", "quantity": 1 }),
        json!({ "item": "\u{7}\u{8}", "quantity": 1 }),
        json!({ "quantity": 1 }),
        json!({ "item": "Menge als Text", "quantity": "5" }),
        json!("keine Zeile"),
        json!({ "item": "Auch gut", "quantity": 0, "quality": 0 }),
    ];
    let parsed = parse_lots(&rows);
    assert_eq!(parsed.rows.iter().map(|r| r.item.as_str()).collect::<Vec<_>>(), ["Gut", "Auch gut"]);
    assert_eq!(parsed.dropped, 10);
}

#[test]
fn texts_lose_control_characters_and_are_cut_to_the_length_of_the_standard() {
    let long = "x".repeat(500);
    let parsed = parse_lots(&[
        json!({ "item": format!("A\u{1b}[31mB{long}"), "quantity": 1, "location": long, "member": long }),
    ]);
    let row = &parsed.rows[0];
    assert!(!row.item.chars().any(char::is_control) && row.item.chars().count() == 200, "{}", row.item);
    assert_eq!((row.location.chars().count(), row.member.chars().count()), (200, 80));
}

#[test]
fn a_missing_or_unknown_unit_means_pieces() {
    let parsed = parse_lots(&[
        json!({ "item": "A", "quantity": 1 }),
        json!({ "item": "B", "quantity": 1, "unit": "Barrel" }),
        json!({ "item": "C", "quantity": 1, "unit": "CSCU" }),
    ]);
    assert_eq!(parsed.rows.iter().map(|r| r.unit).collect::<Vec<_>>(), [Unit::Piece, Unit::Piece, Unit::Cscu]);
}

#[test]
fn the_names_of_the_members_are_private_in_streamer_mode_from_the_moment_they_are_read() {
    let lots = parse_lots(&[json!({ "item": "A", "quantity": 1, "member": "Zorbulon-Lagerhalter" })]);
    assert_eq!(lots.rows[0].member, "Zorbulon-Lagerhalter");
    assert!(crate::streamer::needs_cover("von Zorbulon-Lagerhalter geteilt"), "a member of the stock");
    parse_blueprints(&[json!({ "name": "B", "holders": ["Zorbulon-Planhalter"] })]);
    assert!(crate::streamer::needs_cover("Zorbulon-Planhalter hat ihn"), "a holder of a blueprint");
}

#[test]
fn blueprints_keep_their_holders_and_drop_what_has_no_name() {
    let parsed = parse_blueprints(&[
        json!({ "name": "Arclight Pistol", "holders": ["Pilot", "", 3, "Zweite"] }),
        json!({ "holders": ["x"] }),
        json!({ "name": "P4-AR Rifle" }),
    ]);
    assert_eq!(parsed.dropped, 1);
    assert_eq!(
        parsed.rows[0],
        OrgBlueprint { name: "Arclight Pistol".into(), holders: vec!["Pilot".into(), "Zweite".into()] }
    );
    assert_eq!(parsed.rows[1].holders, Vec::<String>::new());
}

#[test]
fn an_order_is_read_with_its_lines_and_what_is_left_to_deliver() {
    let order = json!({
        "id": "a1", "title": "Erz", "status": "open", "location": "Area18", "due": "2026-12-24", "note": "eilig",
        "items": [{ "item": "Quantanium", "quantity": 10, "unit": "scu", "delivered": 4 }, { "item": "Gold", "quantity": 2, "delivered": 5 }]
    });
    let parsed = parse_orders(&[order]);
    assert_eq!(parsed.dropped, 0);
    let order = &parsed.rows[0];
    assert_eq!((order.status, order.due.as_str(), order.note.as_str()), (OrderStatus::Open, "2026-12-24", "eilig"));
    assert_eq!(order.lines[0].open(), 6.0);
    assert_eq!(order.lines[1].open(), 0.0, "an over-delivery is no negative rest");
}

#[test]
fn a_broken_order_falls_away_and_a_bad_date_only_loses_the_date() {
    let good = json!({ "id": "g", "title": "Gut", "status": "done", "due": "morgen", "items": [] });
    let parsed = parse_orders(&[
        good,
        json!({ "id": "x", "title": "Status", "status": "weird", "items": [] }),
        json!({ "id": "y", "title": "Ohne Zeilen", "status": "open" }),
        json!({ "title": "Ohne Nummer", "status": "open", "items": [] }),
        json!({ "id": "z", "title": "Zeile kaputt", "status": "open", "items": [{ "item": "A", "quantity": -1, "delivered": 0 }] }),
    ]);
    assert_eq!(parsed.rows.len(), 1);
    assert_eq!((parsed.rows[0].id.as_str(), parsed.rows[0].due.as_str()), ("g", ""));
    assert_eq!(parsed.dropped, 4);
}
