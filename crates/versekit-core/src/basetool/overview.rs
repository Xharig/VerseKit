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

//! What the connection page says about the sync (`basetool_page._area_card`,
//! `_decisions`): the lines under each area and the cards that ask the player
//! something. Pure functions of the counts of the last pass and the sync
//! state, so they can be tested without a window.
//!
//! The counts of a pass are an object per area (`Status::counts`), named
//! like the area: `blueprints`, `stock`, `ships`, `demand`, `orgstock`. A
//! missing key means the area did not run, and it says nothing then.

use std::collections::BTreeSet;

use crate::basetool::error::error_text;
use crate::basetool::hold::held_routes;
use crate::basetool::scopes::{
    ROUTE_BLUEPRINTS, ROUTE_SHIPS, ROUTE_STOCK, SCOPES_BLUEPRINTS, SCOPES_DEMAND, SCOPES_HANGAR, SCOPES_ORGSTOCK,
    SCOPES_STOCK, SEND_ROUTES, SETTING_BLUEPRINTS, SETTING_DEMAND, SETTING_ORGSTOCK, SETTING_SHIPS, SETTING_STOCK,
};
use crate::basetool::sync::{Phase, Status};
use crate::formats;
use crate::jsonio::{Map, Value};
use crate::language::{self, Arg, t, tf};

pub const COUNTS_BLUEPRINTS: &str = "blueprints";
pub const COUNTS_STOCK: &str = "stock";
pub const COUNTS_SHIPS: &str = "ships";
pub const COUNTS_DEMAND: &str = "demand";
pub const COUNTS_ORGSTOCK: &str = "orgstock";

/// An area as a card of the page: switch, the rights it needs, title and
/// explanation. In the order of the page.
pub struct AreaCard {
    pub setting: &'static str,
    pub scopes: &'static [&'static str],
    pub title: &'static str,
    pub help: &'static str,
}

pub const CARDS: [AreaCard; 5] = [
    AreaCard {
        setting: SETTING_BLUEPRINTS,
        scopes: SCOPES_BLUEPRINTS,
        title: "s_btv_bauplaene",
        help: "s_btv_bauplaene_h",
    },
    AreaCard { setting: SETTING_STOCK, scopes: SCOPES_STOCK, title: "s_btv_lager", help: "s_btv_lager_h" },
    AreaCard { setting: SETTING_SHIPS, scopes: SCOPES_HANGAR, title: "s_btv_hangar", help: "s_btv_hangar_h" },
    AreaCard { setting: SETTING_DEMAND, scopes: SCOPES_DEMAND, title: "s_btv_bedarf", help: "s_btv_bedarf_h" },
    AreaCard {
        setting: SETTING_ORGSTOCK,
        scopes: SCOPES_ORGSTOCK,
        title: "s_btv_staffellager",
        help: "s_btv_staffellager_h",
    },
];

/// What the unit demand holds, once that area exists.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DemandInfo {
    /// The Basetool does not release the demand to this account.
    pub no_right: bool,
    pub materials: usize,
    pub items: usize,
    /// Orders of the logistician unit, when the member sees them.
    pub orders: Option<usize>,
}

fn field<'a>(map: &'a Map, key: &str) -> Option<&'a Value> {
    map.iter().find(|(k, _)| k == key).map(|(_, v)| v)
}

fn object<'a>(map: &'a Map, key: &str) -> Option<&'a Map> {
    field(map, key).and_then(Value::as_object)
}

/// A whole number of a count; missing or no number is 0.
fn count(map: &Map, key: &str) -> i64 {
    match field(map, key) {
        Some(Value::Int(text)) => text.parse().unwrap_or(0),
        Some(Value::Float(x)) if x.is_finite() => *x as i64,
        Some(Value::Bool(true)) => 1,
        _ => 0,
    }
}

/// How many entries an object (or list) under `key` has.
fn size(map: &Map, key: &str) -> usize {
    match field(map, key) {
        Some(Value::Object(inner)) => inner.len(),
        Some(Value::Array(items)) => items.len(),
        _ => 0,
    }
}

fn texts_of(value: Option<&Value>) -> Vec<String> {
    match value {
        Some(Value::Array(items)) => items.iter().filter_map(|v| v.as_str().map(str::to_string)).collect(),
        _ => Vec::new(),
    }
}

fn n(name: &'static str, value: impl Into<Arg>) -> (&'static str, Arg) {
    (name, value.into())
}

/// The reasons for refusals, at most three, each only once, sorted.
fn reasons(sub: &Map) -> String {
    let all: BTreeSet<&str> =
        object(sub, "rejected").map(|r| r.iter().filter_map(|(_, v)| v.as_str()).collect()).unwrap_or_default();
    all.into_iter().take(3).collect::<Vec<_>>().join(", ")
}

fn refused(lines: &mut Vec<String>, counts: &Map, sub: &Map) {
    let rejected = count(counts, "rejected");
    if rejected != 0 {
        lines.push(tf("s_btv_abgelehnt_n", &[n("n", rejected), n("gruende", reasons(sub))]));
    }
}

/// The lines under an area: what the last pass did, always from the same
/// source, so the player reads from the page that it works.
pub fn area_lines(setting: &str, counts: &Map, state: &Map, demand: Option<&DemandInfo>) -> Vec<String> {
    let mut lines = Vec::new();
    let empty = Map::new();
    match setting {
        SETTING_BLUEPRINTS => {
            let c = object(counts, COUNTS_BLUEPRINTS).unwrap_or(&empty);
            if size(state, "baseline") > 0 {
                let linked = size(state, "links");
                lines.push(tf("s_btv_ueberblick", &[n("n", size(state, "baseline")), n("m", linked)]));
            }
            if field(c, "added").is_some() {
                lines.push(tf(
                    "s_btv_stand_bp",
                    &[
                        n("gesendet", count(c, "added")),
                        n("uebernommen", count(c, "local_added")),
                        n("entfernt", count(c, "removed")),
                    ],
                ));
            }
            if count(c, "unresolved") != 0 {
                lines.push(tf("s_btv_unbekannt", &[n("n", count(c, "unresolved"))]));
            }
        }
        SETTING_STOCK => {
            let sub = object(state, "stock").unwrap_or(&empty);
            let c = object(counts, COUNTS_STOCK).unwrap_or(&empty);
            if field(sub, "server").is_some() {
                lines.push(tf("s_btv_lager_ueberblick", &[n("n", size(sub, "server"))]));
            }
            if !c.is_empty() {
                lines.push(tf(
                    "s_btv_lager_stand",
                    &[n("gesendet", count(c, "sent")), n("uebernommen", count(c, "taken"))],
                ));
                if count(c, "moved") != 0 {
                    lines.push(tf("s_btv_lager_umgezogen", &[n("n", count(c, "moved"))]));
                }
                if count(c, "skipped_location") != 0 {
                    let places = texts_of(object(sub, "skipped").and_then(|s| field(s, "location")));
                    let shown = places.into_iter().take(5).collect::<Vec<_>>().join(", ");
                    lines.push(tf("s_btv_lager_ort", &[n("n", count(c, "skipped_location")), n("orte", shown)]));
                }
                if count(c, "skipped_material") != 0 {
                    lines.push(tf("s_btv_lager_material", &[n("n", count(c, "skipped_material"))]));
                }
                if count(c, "guarded") != 0 {
                    lines.push(tf("s_btv_lager_geschuetzt", &[n("n", count(c, "guarded"))]));
                }
                if count(c, "offers") != 0 {
                    lines.push(tf("s_btv_lager_angebote", &[n("n", count(c, "offers"))]));
                }
                refused(&mut lines, c, sub);
            }
        }
        SETTING_DEMAND => {
            if let Some(info) = demand {
                if info.no_right {
                    lines.push(t("s_btv_bedarf_kein_recht"));
                } else {
                    lines.push(tf(
                        "s_btv_bedarf_stand",
                        &[n("rohstoffe", info.materials), n("gegenstaende", info.items)],
                    ));
                    if let Some(orders) = info.orders {
                        lines.push(tf("s_btv_bedarf_auftraege", &[n("n", orders)]));
                    }
                }
            }
        }
        SETTING_ORGSTOCK => {
            let c = object(counts, COUNTS_ORGSTOCK).unwrap_or(&empty);
            if let Some(code) = field(c, "error").and_then(Value::as_str).filter(|e| !e.is_empty()) {
                lines.push(error_text(code));
            } else if !c.is_empty() {
                lines.push(tf(
                    "s_btv_staffellager_stand",
                    &[n("einheiten", count(c, "units")), n("posten", count(c, "lots"))],
                ));
                if count(c, "logistician") != 0 {
                    lines.push(tf("s_btv_staffellager_logistiker", &[n("n", count(c, "logistician"))]));
                }
            }
        }
        SETTING_SHIPS => {
            let sub = object(state, "ships").unwrap_or(&empty);
            let c = object(counts, COUNTS_SHIPS).unwrap_or(&empty);
            if field(sub, "server").is_some() {
                lines.push(tf("s_btv_hangar_ueberblick", &[n("n", size(sub, "server")), n("m", size(sub, "links"))]));
            }
            if !c.is_empty() {
                lines.push(tf(
                    "s_btv_hangar_stand",
                    &[
                        n("verknuepft", count(c, "linked")),
                        n("angelegt", count(c, "created")),
                        n("uebernommen", count(c, "taken")),
                    ],
                ));
                for (key, text) in [
                    ("unresolved", "s_btv_hangar_unbekannt"),
                    ("detached", "s_btv_hangar_missionen"),
                    ("unit_taken", "s_btv_einheit_uebernommen"),
                    ("unit_refused", "s_btv_einheit_abgelehnt"),
                ] {
                    if count(c, key) != 0 {
                        lines.push(tf(text, &[n("n", count(c, key))]));
                    }
                }
                refused(&mut lines, c, sub);
            }
        }
        _ => {}
    }
    lines
}

/// The send route of an area, if it has one.
pub fn route_of(setting: &str) -> Option<&'static str> {
    match setting {
        SETTING_BLUEPRINTS => Some(ROUTE_BLUEPRINTS),
        SETTING_STOCK => Some(ROUTE_STOCK),
        SETTING_SHIPS => Some(ROUTE_SHIPS),
        _ => None,
    }
}

/// How a sync result reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// Went through.
    Done,
    /// Went through, but something waits for the player.
    Waiting,
    /// Did not go through.
    Failed,
}

/// The line a sync button shows when a pass is over (`pages._storage_sync_result`).
/// `setting` is the area of the page, `None` for the connection page. A
/// held route of that area comes first, then amounts that differ (storage
/// only), then the time of the pass.
pub fn sync_result(status: &Status, state: &Map, setting: Option<&str>, now: f64) -> (String, Verdict) {
    if status.phase != Phase::Ok {
        let code = if status.code.is_empty() { "INTERNAL" } else { status.code.as_str() };
        return (error_text(code), Verdict::Failed);
    }
    let routes: Vec<&str> = match setting {
        Some(setting) => route_of(setting).into_iter().collect(),
        None => SEND_ROUTES.to_vec(),
    };
    let held: Vec<&str> = held_routes(state, now)
        .into_iter()
        .filter(|(route, _)| routes.contains(&route.as_str()))
        .map(|(_, how)| how)
        .collect();
    if held.contains(&"pending") {
        return (t("s_btv_sync_wartet"), Verdict::Waiting);
    }
    if !held.is_empty() {
        return (t("s_btv_sync_abgelaufen"), Verdict::Waiting);
    }
    if matches!(setting, None | Some(SETTING_STOCK)) {
        let conflicts = object(state, "stock").and_then(|s| object(s, "conflicts")).map_or(0, Vec::len);
        if conflicts > 0 {
            let text = if conflicts == 1 {
                t("s_btv_sync_konflikte_eins")
            } else {
                tf("s_btv_sync_konflikte", &[n("n", conflicts)])
            };
            return (text, Verdict::Waiting);
        }
    }
    (tf("s_btv_sync_ok", &[n("zeit", last_sync_text(status))]), Verdict::Done)
}

/// When the last pass went through, in the date format of the language.
pub fn last_sync_text(status: &Status) -> String {
    status.last_sync.map(|at| formats::date_time(language::current(), at)).unwrap_or_default()
}

/// What a card asks, and which buttons answer it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecisionKind {
    /// Removed in the Basetool, the logs still prove them: put back anyway.
    BlueprintsGone,
    /// Removed here: remove in the Basetool too, or take back here.
    BlueprintsRemovedHere,
    /// Different amounts on both sides: mine or theirs.
    StockAmounts,
    /// Ships missing here: remove in the Basetool too, or keep there.
    ShipsMissing,
    /// Ships removed in the Basetool: create them again anyway.
    ShipsGone,
}

/// A card that asks the player something.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecisionCard {
    pub kind: DecisionKind,
    pub title: String,
    pub help: String,
    /// What it is about, in words.
    pub listing: String,
    /// The keys the buttons act on.
    pub keys: Vec<String>,
}

const LISTING_LIMIT: usize = 12;

/// The names of `entries` (key → Basetool key), sorted and cut after twelve.
fn listing(entries: &Map, names: &Map) -> String {
    let mut shown: Vec<String> = entries
        .iter()
        .map(|(key, bt)| {
            let bt = bt.as_str().unwrap_or_default();
            object_name(names, bt).filter(|n| !n.is_empty()).unwrap_or_else(|| key.clone())
        })
        .collect();
    shown.sort();
    let mut text = shown.iter().take(LISTING_LIMIT).cloned().collect::<Vec<_>>().join(", ");
    if shown.len() > LISTING_LIMIT {
        text.push(' ');
        text.push_str(&tf("s_btv_und_mehr", &[n("n", shown.len() - LISTING_LIMIT)]));
    }
    text
}

fn object_name(names: &Map, bt: &str) -> Option<String> {
    field(names, bt).and_then(Value::as_str).map(str::to_string)
}

/// `{bt: name}` of the last state of the Basetool; without a name the key.
fn baseline_names(state: &Map) -> Map {
    object(state, "baseline")
        .map(|baseline| {
            baseline
                .iter()
                .map(|(bt, item)| {
                    let name = item.get("ref").and_then(|r| r.get("name")).and_then(Value::as_str).unwrap_or_default();
                    (bt.clone(), Value::str(if name.is_empty() { bt.as_str() } else { name }))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn display(value: &Value) -> Arg {
    match value {
        Value::Int(text) => text.parse::<i64>().map_or_else(|_| Arg::from(text.as_str()), Arg::from),
        Value::Float(x) => Arg::from(*x),
        Value::Str(s) => Arg::from(s.as_str()),
        other => Arg::from(crate::basetool::error::py_str(other)),
    }
}

/// The amounts that differ, one line per item, twelve at most.
pub fn stock_lines(state: &Map) -> Vec<String> {
    let empty = Map::new();
    let stock = object(state, "stock").unwrap_or(&empty);
    let conflicts = object(stock, "conflicts").unwrap_or(&empty);
    let labels = object(stock, "conflict_names").unwrap_or(&empty);
    let mut sorted: Vec<&(String, Value)> = conflicts.iter().collect();
    sorted.sort_by(|a, b| a.0.cmp(&b.0));
    let mut lines: Vec<String> = sorted
        .iter()
        .take(LISTING_LIMIT)
        .map(|(key, pair)| {
            let (here, there) = match pair {
                Value::Array(items) if items.len() == 2 => (display(&items[0]), display(&items[1])),
                other => (display(other), Arg::from("?")),
            };
            let name = field(labels, key).and_then(Value::as_str).unwrap_or(key.as_str());
            tf("s_btv_lager_konflikt_zeile", &[n("name", name), ("hier", here), ("dort", there)])
        })
        .collect();
    if conflicts.len() > LISTING_LIMIT {
        lines.push(tf("s_btv_und_mehr", &[n("n", conflicts.len() - LISTING_LIMIT)]));
    }
    lines
}

/// The cards that ask something, in the order of the page.
pub fn decision_cards(state: &Map) -> Vec<DecisionCard> {
    let empty = Map::new();
    let mut cards = Vec::new();

    let conflicts = object(state, "conflicts").unwrap_or(&empty);
    // The names of all blueprints are read only when a list needs them.
    let needs_names = !conflicts.is_empty() || object(state, "local_removed").is_some_and(|r| !r.is_empty());
    let names = if needs_names { baseline_names(state) } else { Map::new() };
    if !conflicts.is_empty() {
        cards.push(DecisionCard {
            kind: DecisionKind::BlueprintsGone,
            title: tf("s_btv_konflikte", &[n("n", conflicts.len())]),
            help: t("s_btv_konflikte_h"),
            listing: listing(conflicts, &names),
            keys: conflicts.iter().map(|(k, _)| k.clone()).collect(),
        });
    }

    let removed = object(state, "local_removed").unwrap_or(&empty);
    if !removed.is_empty() {
        cards.push(DecisionCard {
            kind: DecisionKind::BlueprintsRemovedHere,
            title: tf("s_btv_hier_entfernt", &[n("n", removed.len())]),
            help: t("s_btv_hier_entfernt_h"),
            listing: listing(removed, &names),
            keys: removed.iter().map(|(k, _)| k.clone()).collect(),
        });
    }

    let stock = object(state, "stock").unwrap_or(&empty);
    let amounts = object(stock, "conflicts").unwrap_or(&empty);
    if !amounts.is_empty() {
        cards.push(DecisionCard {
            kind: DecisionKind::StockAmounts,
            title: tf("s_btv_lager_konflikte", &[n("n", amounts.len())]),
            help: t("s_btv_lager_konflikte_h"),
            listing: stock_lines(state).join("\n"),
            keys: amounts.iter().map(|(k, _)| k.clone()).collect(),
        });
    }

    let ships = object(state, "ships").unwrap_or(&empty);
    let ship_names = object(ships, "names").unwrap_or(&empty);
    let gone = object(ships, "local_removed").unwrap_or(&empty);
    if !gone.is_empty() {
        let server = object(ships, "server").unwrap_or(&empty);
        let mut shown: Vec<String> = gone
            .iter()
            .map(|(ext, sid)| {
                let on_server = sid
                    .as_str()
                    .and_then(|sid| field(server, sid))
                    .and_then(|s| s.get("shipType"))
                    .and_then(|t| t.get("name"))
                    .and_then(Value::as_str)
                    .filter(|n| !n.is_empty());
                let own = field(ship_names, ext).and_then(Value::as_str).filter(|n| !n.is_empty());
                on_server.or(own).unwrap_or(ext.as_str()).to_string()
            })
            .collect();
        shown.sort();
        cards.push(DecisionCard {
            kind: DecisionKind::ShipsMissing,
            title: tf("s_btv_hangar_weg", &[n("n", gone.len())]),
            help: t("s_btv_hangar_weg_h"),
            listing: shown.join(", "),
            keys: gone.iter().map(|(k, _)| k.clone()).collect(),
        });
    }
    let elsewhere = texts_of(field(ships, "removed_elsewhere"));
    if !elsewhere.is_empty() {
        let mut shown: Vec<String> = elsewhere
            .iter()
            .map(|e| field(ship_names, e).and_then(Value::as_str).unwrap_or(e.as_str()).to_string())
            .collect();
        shown.sort();
        cards.push(DecisionCard {
            kind: DecisionKind::ShipsGone,
            title: tf("s_btv_hangar_dort_weg", &[n("n", elsewhere.len())]),
            help: t("s_btv_hangar_dort_weg_h"),
            listing: shown.join(", "),
            keys: elsewhere,
        });
    }
    cards
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jsonio::loads;

    fn map(text: &str) -> Map {
        match loads(text).expect("json") {
            Value::Object(map) => map,
            other => panic!("kein Objekt: {other:?}"),
        }
    }

    fn lines(setting: &str, counts: &str, state: &str) -> Vec<String> {
        area_lines(setting, &map(counts), &map(state), None)
    }

    /// The lines in the order of 4.12, each only under its condition.
    #[test]
    fn blueprint_lines_follow_4_12() {
        let state = r#"{"baseline": {"a": {}, "b": {}, "c": {}}, "links": {"x": "a"}}"#;
        let counts = r#"{"blueprints": {"added": 3, "local_added": 2, "removed": 1, "unresolved": 4}}"#;
        let got = lines(SETTING_BLUEPRINTS, counts, state);
        assert_eq!(got.len(), 3, "{got:?}");
        assert_eq!(got[0], tf("s_btv_ueberblick", &[n("n", 3), n("m", 1)]));
        assert_eq!(got[1], tf("s_btv_stand_bp", &[n("gesendet", 3), n("uebernommen", 2), n("entfernt", 1)]));
        assert_eq!(got[2], tf("s_btv_unbekannt", &[n("n", 4)]));
        // Nothing synced yet: no lines. 0 sent still counts as a pass.
        assert!(lines(SETTING_BLUEPRINTS, "{}", r#"{"baseline": {}}"#).is_empty());
        let zero = lines(SETTING_BLUEPRINTS, r#"{"blueprints": {"added": 0}}"#, "{}");
        assert_eq!(zero, [tf("s_btv_stand_bp", &[n("gesendet", 0), n("uebernommen", 0), n("entfernt", 0)])]);
    }

    /// Storage: the refusals with at most three reasons, the places cut at five,
    /// the offers last but one.
    #[test]
    fn storage_lines_name_every_event() {
        let state = r#"{"stock": {"server": {"1": {}, "2": {}},
            "skipped": {"location": ["A", "B", "C", "D", "E", "F"]},
            "rejected": {"k1": "STOCK_OFFERED", "k2": "NOT_PERMITTED", "k3": "STOCK_OFFERED", "k4": "ZZZ", "k5": "AAA"}}}"#;
        let counts = r#"{"stock": {"sent": 5, "taken": 6, "moved": 7, "skipped_location": 2, "skipped_material": 3,
            "guarded": 4, "offers": 9, "rejected": 5}}"#;
        let got = lines(SETTING_STOCK, counts, state);
        let expected = vec![
            tf("s_btv_lager_ueberblick", &[n("n", 2)]),
            tf("s_btv_lager_stand", &[n("gesendet", 5), n("uebernommen", 6)]),
            tf("s_btv_lager_umgezogen", &[n("n", 7)]),
            tf("s_btv_lager_ort", &[n("n", 2), n("orte", "A, B, C, D, E")]),
            tf("s_btv_lager_material", &[n("n", 3)]),
            tf("s_btv_lager_geschuetzt", &[n("n", 4)]),
            tf("s_btv_lager_angebote", &[n("n", 9)]),
            tf("s_btv_abgelehnt_n", &[n("n", 5), n("gruende", "AAA, NOT_PERMITTED, STOCK_OFFERED")]),
        ];
        assert_eq!(got, expected);
        // Only the overview when the pass has no counts yet.
        assert_eq!(lines(SETTING_STOCK, "{}", state).len(), 1);
    }

    /// Hangar: lost missions and the unit changes, then the refusals.
    #[test]
    fn hangar_lines_name_every_event() {
        let state = r#"{"ships": {"server": {"s1": {}}, "links": {"e1": "s1"}, "rejected": {"e2": "SHIP_LOCKED"}}}"#;
        let counts = r#"{"ships": {"linked": 1, "created": 2, "taken": 3, "unresolved": 4, "detached": 5,
            "unit_taken": 6, "unit_refused": 7, "rejected": 1}}"#;
        let got = lines(SETTING_SHIPS, counts, state);
        let expected = vec![
            tf("s_btv_hangar_ueberblick", &[n("n", 1), n("m", 1)]),
            tf("s_btv_hangar_stand", &[n("verknuepft", 1), n("angelegt", 2), n("uebernommen", 3)]),
            tf("s_btv_hangar_unbekannt", &[n("n", 4)]),
            tf("s_btv_hangar_missionen", &[n("n", 5)]),
            tf("s_btv_einheit_uebernommen", &[n("n", 6)]),
            tf("s_btv_einheit_abgelehnt", &[n("n", 7)]),
            tf("s_btv_abgelehnt_n", &[n("n", 1), n("gruende", "SHIP_LOCKED")]),
        ];
        assert_eq!(got, expected);
    }

    /// Unit storage and demand: an error replaces the counts, the logistician
    /// line only with logisticians.
    #[test]
    fn unit_storage_and_demand_lines() {
        let got = lines(SETTING_ORGSTOCK, r#"{"orgstock": {"units": 3, "lots": 9, "logistician": 2}}"#, "{}");
        assert_eq!(
            got,
            [
                tf("s_btv_staffellager_stand", &[n("einheiten", 3), n("posten", 9)]),
                tf("s_btv_staffellager_logistiker", &[n("n", 2)])
            ]
        );
        let none = lines(SETTING_ORGSTOCK, r#"{"orgstock": {"units": 0, "lots": 0, "logistician": 0}}"#, "{}");
        assert_eq!(none.len(), 1);
        let failed = lines(SETTING_ORGSTOCK, r#"{"orgstock": {"error": "NOT_PERMITTED"}}"#, "{}");
        assert_eq!(failed, [error_text("NOT_PERMITTED")]);
        let empty = Map::new();
        let info = DemandInfo { no_right: false, materials: 4, items: 5, orders: Some(6) };
        let got = area_lines(SETTING_DEMAND, &empty, &empty, Some(&info));
        assert_eq!(got.len(), 2);
        assert_eq!(got[1], tf("s_btv_bedarf_auftraege", &[n("n", 6)]));
        let denied = DemandInfo { no_right: true, ..info };
        assert_eq!(area_lines(SETTING_DEMAND, &empty, &empty, Some(&denied)), [t("s_btv_bedarf_kein_recht")]);
        assert!(area_lines(SETTING_DEMAND, &empty, &empty, None).is_empty());
    }

    /// The cards that ask: names from the last state of the Basetool, sorted,
    /// twelve at most; every key stays for the buttons.
    #[test]
    fn decision_cards_list_and_cut() {
        let mut baseline = String::new();
        let mut gone = String::new();
        for i in 0..14 {
            baseline.push_str(&format!(r#""bt{i:02}": {{"ref": {{"name": "Name {:02}"}}}},"#, 13 - i));
            gone.push_str(&format!(r#""k{i:02}": "bt{i:02}","#));
        }
        let state = format!(
            r#"{{"baseline": {{{}}}, "conflicts": {{{}"kx": "unbekannt"}}, "local_removed": {{"k00": "bt00"}}}}"#,
            baseline.trim_end_matches(','),
            gone
        );
        let cards = decision_cards(&map(&state));
        assert_eq!(cards.len(), 2, "{cards:?}");
        let first = &cards[0];
        assert_eq!(first.kind, DecisionKind::BlueprintsGone);
        assert_eq!(first.keys.len(), 15, "alle Schlüssel, nicht nur die angezeigten");
        // 15 entries, sorted: "Name 00" … "Name 13" and the key "kx" last.
        let parts: Vec<&str> = first.listing.split(", ").collect();
        assert_eq!(parts.len(), 12);
        assert_eq!(parts[0], "Name 00");
        assert!(first.listing.ends_with(&tf("s_btv_und_mehr", &[n("n", 3)])), "{}", first.listing);
        assert_eq!(cards[1].kind, DecisionKind::BlueprintsRemovedHere);
        assert_eq!(cards[1].listing, "Name 13");
        assert!(decision_cards(&Map::new()).is_empty());
    }

    /// Amounts and ships: lines per item, names from the server before the
    /// own ones, the other ship lists as they are.
    #[test]
    fn stock_and_ship_cards() {
        let state = r#"{"stock": {"conflicts": {"b": [1.5, 2], "a": [3, 4]}, "conflict_names": {"a": "Laranite @ Area18 (Q500)"}},
            "ships": {"names": {"e1": "Mein Falke", "e3": "Dritter", "e5": "Mein Schneider"},
                      "server": {"s2": {"shipType": {"name": "Hornet"}}, "s5": {"shipType": {"name": "Cutlass"}}},
                      "local_removed": {"e1": "s1", "e2": "s2", "e5": "s5", "e9": "s9"},
                      "removed_elsewhere": ["e3", "e4"]}}"#;
        let cards = decision_cards(&map(state));
        let kinds: Vec<DecisionKind> = cards.iter().map(|c| c.kind).collect();
        assert_eq!(kinds, [DecisionKind::StockAmounts, DecisionKind::ShipsMissing, DecisionKind::ShipsGone]);
        let stock: Vec<String> = cards[0].listing.split('\n').map(str::to_string).collect();
        assert_eq!(
            stock[0],
            tf("s_btv_lager_konflikt_zeile", &[n("name", "Laranite @ Area18 (Q500)"), n("hier", 3), n("dort", 4)])
        );
        assert_eq!(stock[1], tf("s_btv_lager_konflikt_zeile", &[n("name", "b"), n("hier", 1.5), n("dort", 2)]));
        // A ship with both names shows the one of the server; with only the
        // own one that; with none the key.
        assert_eq!(cards[1].listing, "Cutlass, Hornet, Mein Falke, e9");
        assert_eq!(cards[2].listing, "Dritter, e4");
        assert_eq!(cards[2].keys, ["e3", "e4"]);
    }

    /// The line a sync button shows: errors, a held route, differing amounts,
    /// and the time at last.
    #[test]
    fn the_sync_result_picks_the_right_line() {
        let ok = Status { phase: Phase::Ok, last_sync: Some(1_000_000.0), ..Status::default() };
        let time = tf("s_btv_sync_ok", &[n("zeit", last_sync_text(&ok))]);
        assert_eq!(sync_result(&ok, &Map::new(), None, 5.0), (time.clone(), Verdict::Done));
        let held = map(r#"{"held": {"/me/stock/changes": {"until": 100.0}}}"#);
        assert_eq!(sync_result(&ok, &held, Some(SETTING_STOCK), 50.0), (t("s_btv_sync_wartet"), Verdict::Waiting));
        assert_eq!(sync_result(&ok, &held, None, 150.0), (t("s_btv_sync_abgelaufen"), Verdict::Waiting));
        // Another page does not see the hold of the storage.
        assert_eq!(sync_result(&ok, &held, Some(SETTING_ORGSTOCK), 50.0).1, Verdict::Done);
        assert_eq!(sync_result(&ok, &held, Some(SETTING_SHIPS), 50.0).1, Verdict::Done);
        let amounts = map(r#"{"stock": {"conflicts": {"a": [1, 2], "b": [1, 2]}}}"#);
        assert_eq!(sync_result(&ok, &amounts, Some(SETTING_STOCK), 5.0).0, tf("s_btv_sync_konflikte", &[n("n", 2)]));
        assert_eq!(sync_result(&ok, &amounts, Some(SETTING_SHIPS), 5.0).1, Verdict::Done);
        let failed = Status { phase: Phase::Error, code: "RATE_LIMITED".into(), ..Status::default() };
        assert_eq!(sync_result(&failed, &Map::new(), None, 5.0), (error_text("RATE_LIMITED"), Verdict::Failed));
        let nameless = Status { phase: Phase::Error, ..Status::default() };
        assert_eq!(sync_result(&nameless, &Map::new(), None, 5.0).0, error_text("INTERNAL"));
        let stopped = Status { phase: Phase::Stopped, code: "account_mismatch".into(), ..Status::default() };
        assert_eq!(sync_result(&stopped, &Map::new(), None, 5.0).1, Verdict::Failed);
    }
}
