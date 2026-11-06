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

//! Personal and shared: how the own stock lots divide at the Basetool, and
//! sharing into the pool of a unit (`stock_split.py`).
//!
//! A lot of `GET /me/stock` carries `split`: the personal part and, per unit
//! the member holds some of in its pool, the amount. Personal plus pools is
//! the amount of the lot, so a shared item never counts twice.
//!
//! | Op | does | `expectedQuantity` |
//! |---|---|---|
//! | `share` | personal → pool of the unit | `split.personal` |
//! | `unshare` | own part of the pool → personal | amount of that pool |
//!
//! Both address the lot like `set-quantity` and always name `orgUnit`; the
//! Basetool never chooses. The fields come from the lot of the Basetool, not
//! from the own row, so the op hits exactly the lot whose split the page shows.
//!
//! Which own row belongs to which lot is told by the identifier of the sync
//! (`stock_sync::identity`). Several rows in one lot: the first ones get the
//! shared part, the rest is personal.
//!
//! ⚠ Without a connection there is no split: everything counts as personal.

use std::collections::{BTreeMap, BTreeSet};

use crate::basetool::connection::get;
use crate::basetool::error::ApiError;
use crate::basetool::hold;
use crate::basetool::scopes::ROUTE_STOCK;
use crate::basetool::stock_sync::{identity, num_value};
use crate::basetool::sync::SyncHandle;
use crate::jsonio::{Map, Value, map_set};
use crate::stock::{parse_number, py_round};
use crate::text::py_strip;

/// The pieces unit of a lot.
pub const PIECE: &str = "PIECE";

/// Which part of the stock a list shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Scope {
    #[default]
    All,
    Personal,
    Shared,
}

/// The two ops.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Share,
    Unshare,
}

impl Kind {
    pub fn op(self) -> &'static str {
        match self {
            Kind::Share => "share",
            Kind::Unshare => "unshare",
        }
    }
}

/// What the page needs to match its rows, from the state of the last sync.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Snapshot {
    /// The lots by identifier.
    pub lots: BTreeMap<String, Value>,
    /// Lower case name (or `uex:<number>`) to the `bt` key of the Basetool.
    pub names: BTreeMap<String, String>,
    /// The `bt` keys that are items.
    pub items: BTreeSet<String>,
    /// The materials the last pass could not resolve, lower case.
    pub left_materials: BTreeSet<String>,
    /// The places the Basetool does not know that the last pass met, lower case.
    pub left_places: BTreeSet<String>,
    /// The lots (by identifier) whose change the Basetool refused in the last pass, with its reason.
    pub refused: BTreeMap<String, String>,
}

/// Why the Basetool does not get an own row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gap {
    /// The row has no place; the Basetool needs one.
    NoPlace,
    /// The place of the row is not in the catalog of the Basetool.
    UnknownPlace,
    /// The Basetool does not know the material or item.
    UnknownPart,
    /// The Basetool refused the change of the lot (`AMBIGUOUS`, `LOCATION_UNKNOWN` …).
    Refused,
}

fn folded(text: &str) -> String {
    py_strip(text).to_lowercase()
}

/// Why the pass leaves an own row out, or `None` when nothing keeps it from the Basetool. The material comes
/// first, as in the pass; `key` is the identifier of the lot the row belongs to, if its material was resolved.
pub fn gap_of(name: &str, place: &str, key: Option<&str>, snap: &Snapshot) -> Option<Gap> {
    if snap.left_materials.contains(&folded(name)) {
        return Some(Gap::UnknownPart);
    }
    let place = folded(place);
    if place.is_empty() {
        Some(Gap::NoPlace)
    } else if snap.left_places.contains(&place) {
        Some(Gap::UnknownPlace)
    } else if key.is_some_and(|key| snap.refused.contains_key(key)) {
        Some(Gap::Refused)
    } else {
        None
    }
}

/// The lots whose change the last pass saw refused: identifier to reason. A lot refused because it was removed
/// elsewhere is a conflict the player decides, not a refusal.
fn refused_lots(sub: &Map) -> BTreeMap<String, String> {
    let Some(Value::Object(entries)) = get(sub, "rejected") else { return BTreeMap::new() };
    entries
        .iter()
        .filter_map(|(key, reason)| Some((key.clone(), reason.as_str()?.to_string())))
        .filter(|(_, reason)| reason != "REMOVED_ELSEWHERE")
        .collect()
}

/// The names the pass left out, lower case, from the list in the state.
fn left_out(sub: &Map, kind: &str) -> BTreeSet<String> {
    let Some(Value::Array(names)) = get(sub, "skipped").and_then(|s| s.get(kind)) else { return BTreeSet::new() };
    names
        .iter()
        .filter_map(Value::as_str)
        .filter(|name| *name != crate::basetool::stock_sync::NO_PLACE)
        .map(folded)
        .filter(|name| !name.is_empty())
        .collect()
}

fn number(value: Option<&Value>) -> f64 {
    match value {
        Some(Value::Float(x)) => *x,
        Some(Value::Int(n)) => n.parse().unwrap_or(0.0),
        Some(Value::Bool(true)) => 1.0,
        Some(Value::Str(s)) => py_strip(s).parse().unwrap_or(0.0),
        _ => 0.0,
    }
}

fn amount_of(block: Option<&Value>) -> f64 {
    number(block.and_then(|b| b.get("amount")))
}

fn truthy_flag(value: Option<&Value>) -> bool {
    value.is_some_and(crate::basetool::error::truthy)
}

/// What the page needs to match, from the stand of the last sync; `None`
/// as long as the stock was never synced.
pub fn snapshot(state: &Map) -> Option<Snapshot> {
    let sub = match get(state, "stock") {
        Some(Value::Object(sub)) if get(sub, "server").is_some() => sub,
        _ => return None,
    };
    let mut names = BTreeMap::new();
    for cache in ["resolved_uex", "resolved", "resolved_items"] {
        if let Some(Value::Object(entries)) = get(sub, cache) {
            for (key, value) in entries {
                if let Value::Str(bt) = value {
                    names.entry(key.clone()).or_insert_with(|| bt.clone());
                }
            }
        }
    }
    let mut lots = BTreeMap::new();
    if let Some(Value::Object(server)) = get(sub, "server") {
        for (_, lot) in server {
            let material = lot.get("material");
            let bt = material.and_then(|m| m.get("bt")).and_then(Value::as_str).unwrap_or_default().to_string();
            let name = py_strip(material.and_then(|m| m.get("name")).and_then(Value::as_str).unwrap_or_default())
                .to_lowercase();
            if !name.is_empty() && !bt.is_empty() {
                names.entry(name.clone()).or_insert_with(|| bt.clone());
            }
            let place = lot.get("location").and_then(|l| l.get("name")).and_then(Value::as_str).unwrap_or_default();
            let quality = number(lot.get("quality")) as i64;
            let key = identity(if bt.is_empty() { &name } else { &bt }, place, quality, truthy_flag(lot.get("stolen")));
            lots.insert(key, lot.clone());
        }
    }
    let mut items = BTreeSet::new();
    if let Some(Value::Object(flags)) = get(sub, "items") {
        for (bt, flag) in flags {
            if truthy_flag(Some(flag)) {
                items.insert(bt.clone());
            }
        }
    }
    Some(Snapshot {
        lots,
        names,
        items,
        left_materials: left_out(sub, "material"),
        left_places: left_out(sub, "location"),
        refused: refused_lots(sub),
    })
}

/// `(personal, [(unit, amount)])` of a lot. Without `split` (an older
/// Basetool) everything is personal.
pub fn split_of(lot: &Value) -> (f64, Vec<(String, f64)>) {
    let Some(split @ Value::Object(_)) = lot.get("split") else {
        return (amount_of(lot.get("quantity")), Vec::new());
    };
    let mut pools = Vec::new();
    if let Some(Value::Array(list)) = split.get("pools") {
        for pool in list {
            let unit = match pool.get("orgUnit").and_then(|u| u.get("id")) {
                Some(Value::Str(s)) => s.clone(),
                Some(other) if truthy_flag(Some(other)) => crate::basetool::error::py_str(other),
                _ => String::new(),
            };
            let amount = amount_of(pool.get("quantity"));
            if !unit.is_empty() && amount > 0.0 {
                pools.push((unit, amount));
            }
        }
    }
    (amount_of(split.get("personal")), pools)
}

/// An own row, as far as matching it to a lot needs it.
#[derive(Clone, Debug, PartialEq)]
pub struct RowRef {
    /// The key the sync resolves the material under: the name, for goods the
    /// UEX key.
    pub lookup: String,
    /// Raw material stock (a quality) or trade stock.
    pub raw: bool,
    pub item: bool,
    pub quality: Option<i64>,
    pub place: String,
    pub stolen: bool,
    pub amount: f64,
}

/// The identifier of the lot at the Basetool an own row belongs to; `None` if
/// its material was never resolved.
pub fn row_key(row: &RowRef, snap: &Snapshot) -> Option<String> {
    let lookup = py_strip(&row.lookup).to_lowercase();
    let bt = snap.names.get(&lookup).filter(|bt| !bt.is_empty())?;
    let mut quality = if row.raw { row.quality.unwrap_or(0) } else { 0 };
    // An item stands at quality 0 at the Basetool, also while the row still
    // lies in the raw stock.
    if row.item || snap.items.contains(bt) {
        quality = 0;
    }
    Some(identity(bt, &row.place, quality, row.stolen))
}

/// The share of a row.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Share {
    pub personal: f64,
    pub shared: f64,
    /// The identifier of its lot at the Basetool, if it has one.
    pub lot: Option<String>,
}

/// Gives every row its part. Without `snap` everything is personal.
pub fn attach(rows: &[RowRef], snap: Option<&Snapshot>) -> Vec<Share> {
    let mut out: Vec<Share> =
        rows.iter().map(|row| Share { personal: row.amount, shared: 0.0, lot: None }).collect();
    let Some(snap) = snap else { return out };
    let mut by_lot: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (at, row) in rows.iter().enumerate() {
        if let Some(key) = row_key(row, snap).filter(|k| snap.lots.contains_key(k)) {
            by_lot.entry(key).or_default().push(at);
        }
    }
    for (key, members) in by_lot {
        let Some(lot) = snap.lots.get(&key) else { continue };
        let (_personal, pools) = split_of(lot);
        let mut left: f64 = pools.iter().map(|(_, a)| a).sum();
        for at in members {
            let shared = rows[at].amount.min(left);
            left -= shared;
            out[at] = Share {
                personal: py_round(rows[at].amount - shared, 3),
                shared: py_round(shared, 3),
                lot: Some(key.clone()),
            };
        }
    }
    out
}

/// A change set that shares or takes back.
fn op(kind: Kind, lot: &Value, unit: &str, amount: f64, expected: f64) -> Value {
    let material = lot.get("material");
    let location = lot.get("location");
    let unit_word = match lot.get("quantity").and_then(|q| q.get("unit")) {
        Some(Value::Str(s)) if !s.is_empty() => s.clone(),
        _ => "SCU".to_string(),
    };
    let quantity = |value: f64| {
        Value::Object(vec![("amount".into(), num_value(value, &unit_word)), ("unit".into(), Value::str(&unit_word))])
    };
    let mut reference = Map::new();
    map_set(&mut reference, "bt", material.and_then(|m| m.get("bt")).cloned().unwrap_or(Value::Null));
    if let Some(name) = material.and_then(|m| m.get("name")).filter(|n| truthy_flag(Some(n)))
        && let Some(text) = name.as_str()
    {
        map_set(&mut reference, "name", Value::str(text.chars().take(200).collect::<String>()));
    }
    let mut place = Map::new();
    if let Some(Value::Object(fields)) = location {
        for (key, value) in fields {
            if (key == "name" || key == "uex") && truthy_flag(Some(value)) {
                map_set(&mut place, key, value.clone());
            }
        }
    }
    let quality = number(lot.get("quality")) as i64;
    Value::Object(vec![
        ("op".into(), Value::str(kind.op())),
        ("opId".into(), Value::str(format!("{}1", &kind.op()[..1]))),
        ("material".into(), Value::Object(reference)),
        ("location".into(), Value::Object(place)),
        ("quality".into(), Value::int(quality)),
        ("stolen".into(), Value::Bool(truthy_flag(lot.get("stolen")))),
        ("orgUnit".into(), Value::Object(vec![("id".into(), Value::str(unit))])),
        ("quantity".into(), quantity(amount)),
        ("expectedQuantity".into(), quantity(expected)),
    ])
}

/// Give into the pool of the unit: the personal part is expected.
pub fn share_op(lot: &Value, unit: &str, amount: f64) -> Value {
    let (personal, _) = split_of(lot);
    op(Kind::Share, lot, unit, amount, personal)
}

/// Take back from the pool of the unit: its amount is expected.
pub fn unshare_op(lot: &Value, unit: &str, amount: f64) -> Value {
    let (_, pools) = split_of(lot);
    let expected = pools.iter().find(|(u, _)| u == unit).map(|(_, a)| *a).unwrap_or(0.0);
    op(Kind::Unshare, lot, unit, amount, expected)
}

/// A typed amount: over 0, at most `limit`, whole for pieces; `None` otherwise.
pub fn check_amount(text: &str, limit: f64, piece: bool) -> Option<f64> {
    let value = parse_number(text)?;
    if !value.is_finite() || value <= 0.0 || value > limit + 1e-9 {
        return None;
    }
    if piece && (value - value.round_ties_even()).abs() > 1e-9 {
        return None;
    }
    Some(value)
}

/// The text key of a refusal of the two ops.
pub fn reason_key(reason: &str) -> Option<&'static str> {
    Some(match reason {
        "VERSION_CONFLICT" => "s_ml_r_konflikt",
        "QUANTITY_ABOVE_EXPECTED" => "s_ml_r_zu_viel",
        "INSUFFICIENT_STOCK" => "s_ml_r_zu_wenig",
        "STOCK_EARMARKED" => "s_ml_r_reserviert",
        "STOCK_OFFERED" => "s_ml_r_angeboten",
        "NOT_A_MEMBER" => "s_ml_r_kein_mitglied",
        "UNIT_MISMATCH" => "s_ml_r_einheit",
        "LOCATION_UNKNOWN" => "s_ml_r_ort",
        "STOLEN_MARKING_DISABLED" => "s_ml_r_gestohlen",
        "UNMATCHED" | "AMBIGUOUS" => "s_ml_r_unbekannt",
        "MASS_CHANGE_PENDING" => "s_ml_r_wartet",
        _ => return None,
    })
}

/// Why an op did not go through.
#[derive(Clone, Debug, PartialEq)]
pub enum Failure {
    /// The Basetool refused it, with its reason.
    Reason(String),
    /// The request failed.
    Api(ApiError),
}

/// Sends the op.
///
/// ⚠ While a stock send waits for confirmation on the same route nothing goes
/// out: every new send would make its link invalid. The remembered state and
/// the link in memory are checked; the sync sets the link before it saves.
pub fn send(sync: &SyncHandle, op: &Value) -> Result<(), Failure> {
    let state = sync.state().unwrap_or_default();
    let pending = hold::held_routes(&state, sync.now()).into_iter().any(|(r, how)| r == ROUTE_STOCK && how == "pending");
    if pending || sync.confirm_link(ROUTE_STOCK).is_some() {
        return Err(Failure::Reason("MASS_CHANGE_PENDING".into()));
    }
    let body = Value::Object(vec![("ops".into(), Value::Array(vec![op.clone()]))]);
    let answer = sync
        .connection()
        .request("POST", ROUTE_STOCK, Some(&body), &[], Some(&crate::basetool::connection::new_idempotency_key()))
        .map_err(Failure::Api)?;
    if let Some(Value::Array(results)) = get(&answer, "results") {
        for entry in results {
            if !matches!(entry, Value::Object(_)) {
                continue;
            }
            let result = entry.get("result").and_then(Value::as_str).unwrap_or_default();
            if result != "applied" && result != "unchanged" {
                let reason = match entry.get("reason") {
                    Some(v) if truthy_flag(Some(v)) => crate::basetool::error::py_str(v),
                    _ if !result.is_empty() => result.to_string(),
                    _ => "UNKNOWN".to_string(),
                };
                return Err(Failure::Reason(reason.chars().take(64).collect()));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jsonio::loads;

    fn value(text: &str) -> Value {
        loads(text).expect("json")
    }

    const LOT: &str = r#"{"key": "L1", "material": {"bt": "bt-ti", "name": "Titanium"},
        "location": {"name": "Area18", "uex": 5, "other": 1}, "quality": 600, "stolen": false,
        "quantity": {"amount": 10, "unit": "SCU"},
        "split": {"personal": {"amount": 7.5, "unit": "SCU"},
                  "pools": [{"orgUnit": {"id": "u1"}, "quantity": {"amount": 2.5, "unit": "SCU"}},
                            {"orgUnit": {"id": "u2"}, "quantity": {"amount": 0, "unit": "SCU"}},
                            {"orgUnit": {}, "quantity": {"amount": 4, "unit": "SCU"}}]}}"#;

    fn row(lookup: &str, quality: Option<i64>, place: &str, amount: f64) -> RowRef {
        RowRef {
            lookup: lookup.into(),
            raw: true,
            item: false,
            quality,
            place: place.into(),
            stolen: false,
            amount,
        }
    }

    fn snap() -> Snapshot {
        let state = value(&format!(
            r#"{{"stock": {{"server": {{"L1": {LOT}}}, "resolved": {{"titanium": "bt-ti"}}, "items": {{}}}}}}"#
        ));
        let Value::Object(state) = state else { panic!("object") };
        snapshot(&state).expect("a snapshot")
    }

    #[test]
    fn a_lot_splits_into_the_personal_part_and_the_pools_with_an_amount() {
        let (personal, pools) = split_of(&value(LOT));
        assert_eq!(personal, 7.5);
        // A pool without an amount and one without a unit do not count.
        assert_eq!(pools, vec![("u1".to_string(), 2.5)]);
        // Without `split` everything is personal.
        let old = value(r#"{"quantity": {"amount": 3, "unit": "SCU"}}"#);
        assert_eq!(split_of(&old), (3.0, Vec::new()));
    }

    fn snap_with_skipped(skipped: &str) -> Snapshot {
        let state = value(&format!(
            r#"{{"stock": {{"server": {{}}, "resolved": {{}}, "items": {{}}, "skipped": {skipped}}}}}"#
        ));
        let Value::Object(state) = state else { panic!("object") };
        snapshot(&state).expect("a snapshot")
    }

    #[test]
    fn a_row_the_pass_leaves_out_says_why_and_the_material_comes_first() {
        let none = crate::basetool::stock_sync::NO_PLACE;
        let snap = snap_with_skipped(&format!(
            r#"{{"material": ["Mystery Ore", " Odd "], "location": ["Nowhere", "{none}"]}}"#
        ));
        assert_eq!(gap_of("mystery ore", "Levski", None, &snap), Some(Gap::UnknownPart), "case does not matter");
        assert_eq!(gap_of(" Odd", "Levski", None, &snap), Some(Gap::UnknownPart), "spaces do not matter");
        assert_eq!(gap_of("Iron", "", None, &snap), Some(Gap::NoPlace));
        assert_eq!(gap_of("Iron", "   ", None, &snap), Some(Gap::NoPlace));
        assert_eq!(gap_of("Iron", " nowhere ", None, &snap), Some(Gap::UnknownPlace));
        assert_eq!(gap_of("Iron", "Levski", None, &snap), None);
        let unknown = gap_of("Mystery Ore", "", None, &snap);
        assert_eq!(unknown, Some(Gap::UnknownPart), "unknown material before missing place");
        // The dash the pass writes for a row without a place is no place of its own.
        assert!(!snap.left_places.contains(crate::basetool::stock_sync::NO_PLACE));
        assert_eq!(gap_of("Iron", "\u{2014}", None, &snap), None);
    }

    #[test]
    fn a_state_without_a_skipped_list_leaves_nothing_out_except_a_row_without_a_place() {
        let snap = snap();
        assert_eq!(gap_of("Titanium", "Area18", None, &snap), None);
        assert_eq!(gap_of("Titanium", "", None, &snap), Some(Gap::NoPlace));
    }

    #[test]
    fn a_lot_the_basetool_refused_is_a_gap_unless_it_is_a_conflict_the_player_decides() {
        let state = value(
            r#"{"stock": {"server": {}, "resolved": {}, "items": {},
                "rejected": {"bt-mirage|orison|0|0": "AMBIGUOUS", "bt-iron|orison|500|0": "REMOVED_ELSEWHERE"}}}"#,
        );
        let Value::Object(state) = state else { panic!("object") };
        let snap = snapshot(&state).expect("a snapshot");
        assert_eq!(gap_of("Mirage", "Orison", Some("bt-mirage|orison|0|0"), &snap), Some(Gap::Refused));
        assert_eq!(gap_of("Mirage", "Orison", Some("bt-other|orison|0|0"), &snap), None, "another lot");
        assert_eq!(gap_of("Mirage", "Orison", None, &snap), None, "a row never resolved has no lot to be refused");
        assert_eq!(gap_of("Iron", "Orison", Some("bt-iron|orison|500|0"), &snap), None, "a conflict is decided elsewhere");
        assert_eq!(snap.refused.get("bt-mirage|orison|0|0").map(String::as_str), Some("AMBIGUOUS"));
    }

    #[test]
    fn no_snapshot_before_the_stock_was_synced() {
        assert_eq!(snapshot(&Map::new()), None);
        let Value::Object(never) = value(r#"{"stock": {"cursor": "c1"}}"#) else { panic!("object") };
        assert_eq!(snapshot(&never), None);
    }

    #[test]
    fn rows_of_one_lot_get_the_shared_part_first_and_the_rest_is_personal() {
        let rows = [
            row("Titanium", Some(600), "area18", 6.0),
            row("titanium", Some(600), " Area18 ", 4.0),
            row("Titanium", Some(500), "Area18", 1.0),
            row("Unknown", Some(600), "Area18", 1.0),
        ];
        let got = attach(&rows, Some(&snap()));
        assert_eq!((got[0].shared, got[0].personal), (2.5, 3.5));
        assert_eq!((got[1].shared, got[1].personal), (0.0, 4.0));
        assert!(got[0].lot.is_some() && got[1].lot == got[0].lot);
        // Another quality or an unresolved material belongs to no lot.
        assert_eq!(got[2], Share { personal: 1.0, shared: 0.0, lot: None });
        assert_eq!(got[3], Share { personal: 1.0, shared: 0.0, lot: None });
        // Without a connection everything is personal.
        let none = attach(&rows, None);
        assert!(none.iter().all(|s| s.shared == 0.0 && s.lot.is_none()));
        assert_eq!(none[0].personal, 6.0);
    }

    #[test]
    fn an_item_stands_at_quality_zero_also_in_the_raw_stock() {
        let state = value(
            r#"{"stock": {"server": {"L2": {"material": {"bt": "bt-gun", "name": "Gun"},
                "location": {"name": "Area18"}, "quality": 0, "stolen": false,
                "quantity": {"amount": 2, "unit": "PIECE"}, "split": {"personal": {"amount": 2}, "pools": []}}},
                "resolved": {}, "items": {"bt-gun": true}}}"#,
        );
        let Value::Object(state) = state else { panic!("object") };
        let snap = snapshot(&state).expect("snapshot");
        // The name comes from the lot itself.
        let got = attach(&[row("gun", Some(300), "Area18", 2.0)], Some(&snap));
        assert!(got[0].lot.is_some(), "{got:?}");
    }

    #[test]
    fn the_ops_expect_what_the_page_showed_and_name_the_unit() {
        let lot = value(LOT);
        let share = share_op(&lot, "u1", 2.0);
        assert_eq!(share.get("op"), Some(&Value::str("share")));
        assert_eq!(share.get("opId"), Some(&Value::str("s1")));
        assert_eq!(share.get("orgUnit"), Some(&value(r#"{"id": "u1"}"#)));
        assert_eq!(share.get("expectedQuantity"), Some(&value(r#"{"amount": 7.5, "unit": "SCU"}"#)));
        assert_eq!(share.get("quantity"), Some(&value(r#"{"amount": 2.0, "unit": "SCU"}"#)));
        // Only name and uex of the place, the name of the material cut to the field.
        assert_eq!(share.get("location"), Some(&value(r#"{"name": "Area18", "uex": 5}"#)));
        assert_eq!(share.get("material"), Some(&value(r#"{"bt": "bt-ti", "name": "Titanium"}"#)));
        assert_eq!(share.get("quality"), Some(&Value::int(600)));
        let back = unshare_op(&lot, "u1", 1.0);
        assert_eq!(back.get("opId"), Some(&Value::str("u1")));
        assert_eq!(back.get("expectedQuantity"), Some(&value(r#"{"amount": 2.5, "unit": "SCU"}"#)));
        // A unit without a pool expects 0.
        let none = unshare_op(&lot, "u9", 1.0);
        assert_eq!(none.get("expectedQuantity"), Some(&value(r#"{"amount": 0.0, "unit": "SCU"}"#)));
    }

    #[test]
    fn pieces_go_out_whole() {
        let lot = value(
            r#"{"material": {"bt": "b", "name": "N"}, "location": {"name": "A"}, "quality": 0,
                "quantity": {"amount": 5, "unit": "PIECE"},
                "split": {"personal": {"amount": 5}, "pools": []}}"#,
        );
        let op = share_op(&lot, "u1", 2.0);
        assert_eq!(op.get("quantity"), Some(&value(r#"{"amount": 2, "unit": "PIECE"}"#)));
        assert_eq!(op.get("expectedQuantity"), Some(&value(r#"{"amount": 5, "unit": "PIECE"}"#)));
    }

    #[test]
    fn a_typed_amount_is_over_zero_within_the_limit_and_whole_for_pieces() {
        assert_eq!(check_amount("2,5", 7.5, false), Some(2.5));
        assert_eq!(check_amount("7.5", 7.5, false), Some(7.5));
        assert_eq!(check_amount("7.6", 7.5, false), None);
        assert_eq!(check_amount("0", 7.5, false), None);
        assert_eq!(check_amount("-1", 7.5, false), None);
        assert_eq!(check_amount("abc", 7.5, false), None);
        assert_eq!(check_amount("", 7.5, false), None);
        assert_eq!(check_amount("2", 5.0, true), Some(2.0));
        assert_eq!(check_amount("2,5", 5.0, true), None);
    }

    #[test]
    fn every_known_refusal_has_a_text_and_an_unknown_one_none() {
        for reason in [
            "VERSION_CONFLICT",
            "QUANTITY_ABOVE_EXPECTED",
            "INSUFFICIENT_STOCK",
            "STOCK_EARMARKED",
            "STOCK_OFFERED",
            "NOT_A_MEMBER",
            "UNIT_MISMATCH",
            "LOCATION_UNKNOWN",
            "STOLEN_MARKING_DISABLED",
            "UNMATCHED",
            "AMBIGUOUS",
            "MASS_CHANGE_PENDING",
        ] {
            assert!(reason_key(reason).is_some_and(|key| crate::language::t(key) != key), "{reason}");
        }
        assert_eq!(reason_key("SOMETHING"), None);
    }
}

