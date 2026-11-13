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

//! The refinery jobs against the refinery orders of the Basetool, without the net: the orders of the feed, the
//! operations a pass sends and what it takes over into the own jobs.
//!
//! **Both ways.** A job read at the terminal with its refinery and yield lines becomes an order over there under an
//! own key ([`client_key`]); a later reading with another end moves its timer. An order created in the web (or by
//! another installation) becomes a job here, with its end, so the overlay reports it; it leaves again once it is
//! closed or gone over there.
//!
//! **Picked up only closes.** Booking the yield into the own stock marks the job picked up; the pass sends a
//! `pickup` without store lines. The stock reaches the Basetool through the stock sync alone, so nothing is
//! booked twice.
//!
//! ⚠ Ids and keys are opaque: stored and sent back, never taken apart. The game shows no input amount on a running
//! job, so an order is created with `inputQuantity` 0 and the yield as `outputQuantity` (cSCU are the Basetool's
//! units, 100 to the SCU).

use std::collections::{BTreeMap, BTreeSet};

use crate::jsonio::{Map, Value, map_set};
use crate::refinery_jobs::{self, Good};

/// The feed of the own orders.
pub const ROUTE_READ: &str = "/me/refinery-orders";
/// Most operations in one change set.
pub const MAX_OPS: usize = 500;
/// A timer that differs by less than this from the one over there is not sent again: a reading gives minutes.
pub const TIMER_SLACK: f64 = 120.0;
/// The states of an order that still runs or waits for its pickup.
pub const OPEN_STATES: [&str; 2] = ["OPEN", "IN_PROGRESS"];

/// An order of the feed, as far as the sync needs it.
#[derive(Clone, Debug, PartialEq)]
pub struct Order {
    pub id: String,
    /// The own key, only on orders a Verse-Kit installation created.
    pub client_key: Option<String>,
    pub status: String,
    pub ends_at: Option<f64>,
    pub game_data: i64,
    pub place: String,
    /// The output material of each good and its yield in units.
    pub outputs: Vec<(String, i64)>,
}

impl Order {
    pub fn open(&self) -> bool {
        OPEN_STATES.contains(&self.status.as_str())
    }
}

fn whole(value: Option<&Value>) -> Option<i64> {
    match value {
        Some(Value::Int(text)) => text.parse().ok(),
        Some(Value::Float(x)) if x.is_finite() => Some(*x as i64),
        _ => None,
    }
}

fn text_at(value: &Value, path: &[&str]) -> String {
    let mut at = Some(value);
    for key in path {
        at = at.and_then(|v| v.get(key));
    }
    at.and_then(Value::as_str).map(|s| crate::text::py_strip(s).to_string()).unwrap_or_default()
}

/// An order of the feed; one without id is worthless.
pub fn parse_order(item: &Value) -> Option<Order> {
    let id = text_at(item, &["id"]);
    if id.is_empty() {
        return None;
    }
    let key = text_at(item, &["clientKey"]);
    let outputs = match item.get("goods") {
        Some(Value::Array(goods)) => goods
            .iter()
            .map(|g| (text_at(g, &["output", "name"]), whole(g.get("outputQuantity")).unwrap_or(0)))
            .filter(|(name, _)| !name.is_empty())
            .collect(),
        _ => Vec::new(),
    };
    Some(Order {
        id,
        client_key: (!key.is_empty()).then_some(key),
        status: text_at(item, &["status"]),
        ends_at: rfc3339(&text_at(item, &["endsAt"])),
        game_data: whole(item.get("versions").and_then(|v| v.get("gameData"))).unwrap_or(0),
        place: text_at(item, &["location", "name"]),
        outputs,
    })
}

/// Days since 1970 of a date of the Gregorian calendar.
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let yoe = year - era * 400;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// An RFC 3339 time (`2026-11-08T14:30:00Z`, fractions and an offset allowed) as seconds since 1970.
pub fn rfc3339(text: &str) -> Option<f64> {
    let bytes = text.as_bytes();
    if bytes.len() < 20 || !matches!(bytes[10], b'T' | b't' | b' ') {
        return None;
    }
    let num = |from: usize, to: usize| -> Option<i64> {
        let part = text.get(from..to)?;
        part.bytes().all(|b| b.is_ascii_digit()).then(|| part.parse().ok()).flatten()
    };
    let (year, month, day) = (num(0, 4)?, num(5, 7)?, num(8, 10)?);
    let (hour, minute, second) = (num(11, 13)?, num(14, 16)?, num(17, 19)?);
    if bytes[4] != b'-' || bytes[7] != b'-' || bytes[13] != b':' || bytes[16] != b':' {
        return None;
    }
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) || hour > 23 || minute > 59 || second > 60 {
        return None;
    }
    let mut rest = &text[19..];
    let mut fraction = 0.0;
    if let Some(digits) = rest.strip_prefix('.') {
        let end = digits.bytes().take_while(u8::is_ascii_digit).count();
        if end == 0 {
            return None;
        }
        fraction = format!("0.{}", &digits[..end]).parse().ok()?;
        rest = &digits[end..];
    }
    let offset = match rest {
        "Z" | "z" => 0,
        _ if rest.len() == 6 && matches!(rest.as_bytes()[0], b'+' | b'-') && rest.as_bytes()[3] == b':' => {
            let hours = rest.get(1..3)?.parse::<i64>().ok()?;
            let minutes = rest.get(4..6)?.parse::<i64>().ok()?;
            let sign = if rest.starts_with('-') { -1 } else { 1 };
            sign * (hours * 3600 + minutes * 60)
        }
        _ => return None,
    };
    let seconds = days_from_civil(year, month, day) * 86_400 + hour * 3600 + minute * 60 + second - offset;
    Some(seconds as f64 + fraction)
}

/// Seconds since 1970 as RFC 3339 in UTC, to the second.
pub fn rfc3339_text(seconds: f64) -> String {
    crate::export::utc_text(seconds.round() as i64, false)
}

/// A new own key for a job's order: random, so two jobs never share one, whatever their place, materials and end.
/// It is written into the job before its order is created and kept with it from then on; the same job read again is
/// the same job of the store and keeps it.
pub fn client_key() -> String {
    format!("vk-rf-{}", crate::basetool::connection::new_idempotency_key().replace('-', ""))
}

/// The raw materials to try for a refined one: `Gold` comes from `Gold (Ore)` or `Gold (Raw)`. The Basetool
/// resolves them exactly; the first that resolves counts.
pub fn raw_candidates(material: &str) -> [String; 2] {
    let name = crate::text::py_strip(material);
    [format!("{name} (Ore)"), format!("{name} (Raw)")]
}

/// What a pass gives the plan.
pub struct Inputs<'a> {
    pub jobs: &'a [Map],
    /// The open orders over there by id.
    pub server: &'a BTreeMap<String, Order>,
    /// Own key → order id; an empty id is an order that exists over there but was never seen.
    pub links: &'a BTreeMap<String, String>,
    /// The Basetool's places: lower-case name → `{name, …}`.
    pub places: &'a Map,
    /// Refined material (lower case) → the Basetool key of its raw material.
    pub raw: &'a BTreeMap<String, String>,
    /// The unit new orders belong to; `None` creates nothing.
    pub unit: Option<&'a str>,
    pub now: f64,
}

/// What a pass sends and why something stays here.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Plan {
    /// Key of the operation (the own key, or the order id of an order from over there) and the operation.
    pub ops: Vec<(String, Value)>,
    /// Index of a job and the key it gets before its order is created.
    pub new_keys: Vec<(usize, String)>,
    pub creates: usize,
    pub updates: usize,
    pub pickups: usize,
    /// Jobs that cannot become an order: no refinery the Basetool knows, a material without a raw one, no unit.
    pub no_place: usize,
    pub no_material: usize,
    pub no_unit: usize,
}

fn versions(order: &Order) -> Value {
    Value::Object(vec![("gameData".into(), Value::int(order.game_data))])
}

fn op(pairs: Vec<(&str, Value)>) -> Value {
    Value::Object(pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
}

/// The goods of a create, or `None` when one material has no raw one.
fn goods_of(goods: &[Good], raw: &BTreeMap<String, String>) -> Option<Vec<Value>> {
    goods
        .iter()
        .map(|good| {
            let bt = raw.get(&good.material.to_lowercase())?;
            Some(op(vec![
                ("input", op(vec![("bt", Value::str(bt))])),
                ("inputQuantity", Value::int(0)),
                ("outputQuantity", Value::int(good.amount)),
                ("quality", Value::int(good.quality)),
            ]))
        })
        .collect()
}

/// The operations of a pass.
pub fn plan(inputs: &Inputs<'_>) -> Plan {
    let mut out = Plan::default();
    for (index, job) in inputs.jobs.iter().enumerate() {
        let from_there = refinery_jobs::text(job, refinery_jobs::ORDER);
        let own_key = refinery_jobs::text(job, refinery_jobs::KEY);
        let picked = refinery_jobs::picked_up(job);
        let ends = refinery_jobs::ends(job);
        // The order this job stands for, if it is known and open over there.
        let order_id = if !from_there.is_empty() {
            Some(from_there.clone())
        } else {
            inputs.links.get(&own_key).filter(|id| !id.is_empty()).cloned()
        };
        if let Some(order) = order_id.as_ref().and_then(|id| inputs.server.get(id)).filter(|o| o.open()) {
            let key = if from_there.is_empty() { own_key.clone() } else { order.id.clone() };
            if picked {
                out.ops.push((
                    key.clone(),
                    op(vec![
                        ("opId", Value::str(&key)),
                        ("op", Value::str("pickup")),
                        ("id", Value::str(&order.id)),
                        ("versions", versions(order)),
                    ]),
                ));
                out.pickups += 1;
            } else if from_there.is_empty()
                && ends.is_finite()
                && order.ends_at.is_none_or(|there| (there - ends).abs() > TIMER_SLACK)
            {
                out.ops.push((
                    key.clone(),
                    op(vec![
                        ("opId", Value::str(&key)),
                        ("op", Value::str("update")),
                        ("id", Value::str(&order.id)),
                        ("versions", versions(order)),
                        ("endsAt", Value::str(rfc3339_text(ends))),
                    ]),
                ));
                out.updates += 1;
            }
            continue;
        }
        // Only own jobs become orders, never one that is picked up already or whose order exists.
        if !from_there.is_empty() || picked || (!own_key.is_empty() && inputs.links.contains_key(&own_key)) {
            continue;
        }
        let goods = refinery_jobs::goods(job);
        let place = refinery_jobs::text(job, refinery_jobs::PLACE);
        if goods.is_empty() || !ends.is_finite() {
            continue;
        }
        let Some(place) = inputs
            .places
            .iter()
            .find(|(k, _)| *k == crate::text::py_strip(&place).to_lowercase() && !place.is_empty())
            .and_then(|(_, v)| v.get("name").and_then(Value::as_str))
        else {
            out.no_place += 1;
            continue;
        };
        let Some(goods) = goods_of(&goods, inputs.raw) else {
            out.no_material += 1;
            continue;
        };
        let Some(unit) = inputs.unit else {
            out.no_unit += 1;
            continue;
        };
        let key = if own_key.is_empty() {
            let key = client_key();
            out.new_keys.push((index, key.clone()));
            key
        } else {
            own_key
        };
        out.ops.push((
            key.clone(),
            op(vec![
                ("opId", Value::str(&key)),
                ("op", Value::str("create")),
                ("clientKey", Value::str(&key)),
                ("orgUnit", op(vec![("id", Value::str(unit))])),
                ("location", op(vec![("name", Value::str(place))])),
                ("status", Value::str("IN_PROGRESS")),
                ("endsAt", Value::str(rfc3339_text(ends))),
                ("goods", Value::Array(goods)),
            ]),
        ));
        out.creates += 1;
    }
    out
}

/// The change sets of a plan, at most [`MAX_OPS`] operations each.
pub fn change_sets(plan: &Plan) -> Vec<crate::basetool::hold::ChangeSet> {
    plan.ops
        .chunks(MAX_OPS)
        .map(|chunk| {
            let ops = Value::Array(chunk.iter().map(|(_, op)| op.clone()).collect());
            (op(vec![("ops", ops)]), chunk.iter().map(|(key, _)| key.clone()).collect())
        })
        .collect()
}

/// What taking the orders over did to the jobs.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Taken {
    pub added: usize,
    pub changed: usize,
    pub dropped: usize,
}

impl Taken {
    pub fn any(&self) -> bool {
        self.added + self.changed + self.dropped > 0
    }
}

/// The sum of an order's yield as the jobs keep it (`total`, a text).
fn total_of(order: &Order) -> String {
    let sum: i64 = order.outputs.iter().map(|(_, amount)| amount).sum();
    if sum > 0 { sum.to_string() } else { String::new() }
}

fn set_if(job: &mut Map, key: &str, value: Value) -> bool {
    if job.iter().any(|(k, v)| k == key && crate::jsonio::py_eq_value(v, &value)) {
        return false;
    }
    map_set(job, key, value);
    true
}

/// The orders over there that are no own job become jobs here, with their end; a job of an order that is no
/// longer open over there leaves. `own_keys` are the keys of the own jobs (an order with one of them is the job
/// itself). A job taken over whose end lies behind `now` is marked as reported: it is no news.
pub fn take_over(jobs: &mut Vec<Map>, server: &BTreeMap<String, Order>, own_keys: &BTreeSet<String>, now: f64) -> Taken {
    let mut taken = Taken::default();
    let before = jobs.len();
    jobs.retain(|job| {
        let id = refinery_jobs::text(job, refinery_jobs::ORDER);
        id.is_empty() || server.get(&id).is_some_and(Order::open)
    });
    taken.dropped = before - jobs.len();
    for order in server.values().filter(|o| o.open()) {
        if order.client_key.as_ref().is_some_and(|k| own_keys.contains(k)) {
            continue;
        }
        let Some(ends) = order.ends_at else { continue };
        let materials: Vec<Value> = {
            let mut seen: Vec<&str> = Vec::new();
            for (name, _) in &order.outputs {
                if !seen.contains(&name.as_str()) {
                    seen.push(name);
                }
            }
            seen.into_iter().map(Value::str).collect()
        };
        let at = jobs.iter().position(|job| refinery_jobs::text(job, refinery_jobs::ORDER) == order.id);
        match at {
            Some(at) => {
                let job = &mut jobs[at];
                let mut changed = set_if(job, "materials", Value::Array(materials));
                changed |= set_if(job, "total", Value::str(total_of(order)));
                if (refinery_jobs::ends(job) - ends).abs() > 0.5 {
                    map_set(job, "ends", Value::Float(ends));
                    // A later end is news again.
                    if ends > now {
                        job.retain(|(k, _)| k != "notified");
                    }
                    changed = true;
                }
                if !order.place.is_empty() {
                    changed |= set_if(job, refinery_jobs::PLACE, Value::str(&order.place));
                }
                taken.changed += usize::from(changed);
            }
            None => {
                let mut job = vec![
                    ("materials".to_string(), Value::Array(materials)),
                    ("total".to_string(), Value::str(total_of(order))),
                    ("ends".to_string(), Value::Float(ends)),
                    (refinery_jobs::ORDER.to_string(), Value::str(&order.id)),
                ];
                if !order.place.is_empty() {
                    job.push((refinery_jobs::PLACE.to_string(), Value::str(&order.place)));
                }
                if ends <= now {
                    job.push(("notified".to_string(), Value::Bool(true)));
                }
                jobs.push(job);
                taken.added += 1;
            }
        }
    }
    if taken.any() {
        jobs.sort_by(|a, b| refinery_jobs::ends(a).total_cmp(&refinery_jobs::ends(b)));
    }
    taken
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jsonio;

    fn map(text: &str) -> Map {
        match jsonio::loads(text).expect("Beispiel") {
            Value::Object(map) => map,
            other => panic!("{other:?}"),
        }
    }

    fn order(id: &str, key: Option<&str>, status: &str, ends: f64, game_data: i64) -> Order {
        Order {
            id: id.into(),
            client_key: key.map(str::to_string),
            status: status.into(),
            ends_at: Some(ends),
            game_data,
            place: "CRU-L1 Ambitious Dream Station".into(),
            outputs: vec![("Gold".into(), 1200), ("Iron".into(), 300)],
        }
    }

    #[test]
    fn times_read_and_written_in_utc() {
        assert_eq!(rfc3339("1970-01-01T00:00:00Z"), Some(0.0));
        assert_eq!(rfc3339("2026-11-08T14:30:00Z"), Some(1_794_148_200.0));
        assert_eq!(rfc3339("2026-11-08T15:30:00.250+01:00"), Some(1_794_148_200.25));
        assert_eq!(rfc3339_text(1_794_148_200.4), "2026-11-08T14:30:00Z");
        for wrong in ["", "2026-11-08", "2026-11-08T14:30:00", "2026-13-08T14:30:00Z", "2026-11-08T14:30:00.Z"] {
            assert_eq!(rfc3339(wrong), None, "{wrong}");
        }
    }

    #[test]
    fn an_order_of_the_feed() {
        let item = jsonio::loads(
            r#"{"id": "o1", "clientKey": "vk-rf-1", "status": "IN_PROGRESS", "endsAt": "2026-11-08T14:30:00Z",
                "versions": {"gameData": 3, "finances": 0}, "location": {"name": "ARC-L1"},
                "goods": [{"id": "g1", "output": {"bt": "b", "name": "Gold"}, "outputQuantity": 900}]}"#,
        )
        .expect("Beispiel");
        let parsed = parse_order(&item).expect("order");
        assert_eq!(parsed.client_key.as_deref(), Some("vk-rf-1"));
        assert_eq!(parsed.game_data, 3);
        assert_eq!(parsed.outputs, [("Gold".to_string(), 900)]);
        assert!(parsed.open());
        assert_eq!(parse_order(&jsonio::loads(r#"{"status": "OPEN"}"#).expect("x")), None);
    }

    #[test]
    fn two_jobs_never_share_a_key_and_a_kept_key_stays() {
        // Two jobs alike but for their end, around the old boundaries of ten minutes: each its own key.
        let jobs: Vec<Map> = ["5999.0", "6001.0", "6002.0"]
            .iter()
            .map(|end| {
                let mut job = map(READ);
                map_set(&mut job, "ends", Value::Float(end.parse().expect("number")));
                job
            })
            .collect();
        let (server, links, places, raw) = (BTreeMap::new(), BTreeMap::new(), places(), raw());
        let made =
            plan(&Inputs { jobs: &jobs, server: &server, links: &links, places: &places, raw: &raw, unit: Some("u1"), now: 0.0 });
        let keys: BTreeSet<&String> = made.new_keys.iter().map(|(_, k)| k).collect();
        assert_eq!(keys.len(), 3, "{keys:?}");
        assert!(keys.iter().all(|k| k.starts_with("vk-rf-") && k.len() <= 128));
        // A job with its key keeps it.
        let mut kept = jobs[0].clone();
        map_set(&mut kept, refinery_jobs::KEY, Value::str("vk-rf-fest"));
        let again = plan(&Inputs {
            jobs: &[kept],
            server: &server,
            links: &links,
            places: &places,
            raw: &raw,
            unit: Some("u1"),
            now: 0.0,
        });
        assert_eq!((again.ops[0].0.as_str(), again.new_keys.len()), ("vk-rf-fest", 0));
    }

    fn places() -> Map {
        map(r#"{"cru-l1 ambitious dream station": {"name": "CRU-L1 Ambitious Dream Station"}}"#)
    }

    fn raw() -> BTreeMap<String, String> {
        [("gold".to_string(), "bt-gold-ore".to_string()), ("iron".to_string(), "bt-iron-ore".to_string())].into()
    }

    const READ: &str = r#"{"materials": ["Gold", "Iron"], "total": "15", "ends": 10000.0,
        "ort": "CRU-L1 Ambitious Dream Station",
        "gueter": [{"material": "Gold", "qualitaet": 512, "menge": 1200}, {"material": "Iron", "qualitaet": 300, "menge": 300}]}"#;

    #[test]
    fn a_read_job_becomes_an_order_once_and_only_with_place_material_and_unit() {
        let jobs = vec![map(READ)];
        let (server, links) = (BTreeMap::new(), BTreeMap::new());
        let (places, raw) = (places(), raw());
        let inputs =
            Inputs { jobs: &jobs, server: &server, links: &links, places: &places, raw: &raw, unit: Some("u1"), now: 0.0 };
        let made = plan(&inputs);
        assert_eq!(made.creates, 1);
        let (key, op) = &made.ops[0];
        assert_eq!(made.new_keys, [(0, key.clone())]);
        assert_eq!(op.get("op").and_then(Value::as_str), Some("create"));
        assert_eq!(op.get("clientKey").and_then(Value::as_str), Some(key.as_str()));
        assert_eq!(op.get("endsAt").and_then(Value::as_str), Some("1970-01-01T02:46:40Z"));
        let goods = op.get("goods").expect("goods");
        assert_eq!(
            jsonio::dumps(goods, None, false),
            r#"[{"input": {"bt": "bt-gold-ore"}, "inputQuantity": 0, "outputQuantity": 1200, "quality": 512}, {"input": {"bt": "bt-iron-ore"}, "inputQuantity": 0, "outputQuantity": 300, "quality": 300}]"#
        );
        // No unit, an unknown place, a material without raw one: nothing goes out, the reason is counted.
        assert_eq!(plan(&Inputs { unit: None, ..inputs }).no_unit, 1);
        let nowhere = Map::new();
        assert_eq!(plan(&Inputs { places: &nowhere, ..inputs }).no_place, 1);
        let gold_only: BTreeMap<String, String> = [("gold".to_string(), "bt-gold-ore".to_string())].into();
        assert_eq!(plan(&Inputs { raw: &gold_only, ..inputs }).no_material, 1);
        // Linked (also to an order never seen): never created again. Picked up before it was ever sent: neither.
        let mut keyed = jobs.clone();
        map_set(&mut keyed[0], refinery_jobs::KEY, Value::str(key));
        let unseen: BTreeMap<String, String> = [(key.clone(), String::new())].into();
        assert!(plan(&Inputs { jobs: &keyed, links: &unseen, ..inputs }).ops.is_empty());
        let mut picked = jobs.clone();
        map_set(&mut picked[0], refinery_jobs::PICKED_UP, Value::Float(5.0));
        assert!(plan(&Inputs { jobs: &picked, ..inputs }).ops.is_empty());
        // A key kept but the create refused: the same key goes out again (the pacing keeps it back while it is
        // the same operation).
        let again = plan(&Inputs { jobs: &keyed, ..inputs });
        assert_eq!((again.creates, again.new_keys.len()), (1, 0));
    }

    #[test]
    fn a_linked_job_moves_its_timer_and_a_picked_up_one_closes_its_order() {
        let mut jobs = vec![map(READ)];
        map_set(&mut jobs[0], refinery_jobs::KEY, Value::str("k1"));
        let links: BTreeMap<String, String> = [("k1".to_string(), "o1".to_string())].into();
        let mut server: BTreeMap<String, Order> =
            [("o1".to_string(), order("o1", Some("k1"), "IN_PROGRESS", 10_060.0, 4))].into();
        let (places, raw) = (places(), raw());
        let make = |jobs: &[Map], server: &BTreeMap<String, Order>| {
            plan(&Inputs { jobs, server, links: &links, places: &places, raw: &raw, unit: Some("u1"), now: 0.0 })
        };
        assert!(make(&jobs, &server).ops.is_empty(), "a minute apart is the same timer");
        server.get_mut("o1").expect("o1").ends_at = Some(9_000.0);
        let moved = make(&jobs, &server);
        assert_eq!(moved.updates, 1);
        assert_eq!(
            jsonio::dumps(&moved.ops[0].1, None, false),
            r#"{"opId": "k1", "op": "update", "id": "o1", "versions": {"gameData": 4}, "endsAt": "1970-01-01T02:46:40Z"}"#
        );
        map_set(&mut jobs[0], refinery_jobs::PICKED_UP, Value::Float(5.0));
        let picked = make(&jobs, &server);
        assert_eq!((picked.pickups, picked.updates), (1, 0));
        assert_eq!(
            jsonio::dumps(&picked.ops[0].1, None, false),
            r#"{"opId": "k1", "op": "pickup", "id": "o1", "versions": {"gameData": 4}}"#,
            "no store lines: the stock sync books the stock"
        );
        // Closed over there: nothing more.
        server.get_mut("o1").expect("o1").status = "COMPLETED".into();
        assert!(make(&jobs, &server).ops.is_empty());
    }

    #[test]
    fn orders_from_over_there_come_and_go_and_only_their_pickup_is_sent() {
        let mut server: BTreeMap<String, Order> = [
            ("o1".to_string(), order("o1", None, "IN_PROGRESS", 500.0, 1)),
            ("o2".to_string(), order("o2", Some("mine"), "IN_PROGRESS", 600.0, 1)),
            ("o3".to_string(), order("o3", None, "OPEN", 50.0, 1)),
        ]
        .into();
        let own: BTreeSet<String> = ["mine".to_string()].into();
        let mut jobs: Vec<Map> = Vec::new();
        let taken = take_over(&mut jobs, &server, &own, 100.0);
        assert_eq!(taken, Taken { added: 2, changed: 0, dropped: 0 }, "the own order is the own job");
        assert_eq!(refinery_jobs::text(&jobs[0], refinery_jobs::ORDER), "o3");
        assert!(jobs[0].iter().any(|(k, v)| k == "notified" && *v == Value::Bool(true)), "over already: no news");
        assert!(!jobs[1].iter().any(|(k, _)| k == "notified"));
        assert_eq!(refinery_jobs::text(&jobs[1], "total"), "1500");
        // Nothing new: nothing changes.
        assert!(!take_over(&mut jobs, &server, &own, 100.0).any());
        // A later end; one closed over there leaves.
        server.get_mut("o1").expect("o1").ends_at = Some(900.0);
        server.get_mut("o3").expect("o3").status = "CANCELED".into();
        assert_eq!(take_over(&mut jobs, &server, &own, 100.0), Taken { added: 0, changed: 1, dropped: 1 });
        assert_eq!(jobs.len(), 1);
        // Picked up here: the order over there is closed by its id.
        map_set(&mut jobs[0], refinery_jobs::PICKED_UP, Value::Float(950.0));
        let (links, places, raw) = (BTreeMap::new(), places(), raw());
        let made = plan(&Inputs {
            jobs: &jobs,
            server: &server,
            links: &links,
            places: &places,
            raw: &raw,
            unit: Some("u1"),
            now: 1000.0,
        });
        assert_eq!(made.ops.len(), 1);
        assert_eq!(made.ops[0].0, "o1");
        assert_eq!(made.ops[0].1.get("op").and_then(Value::as_str), Some("pickup"));
    }

    #[test]
    fn change_sets_hold_at_most_five_hundred_operations() {
        let ops: Vec<(String, Value)> = (0..1001).map(|i| (format!("k{i}"), Value::Null)).collect();
        let sets = change_sets(&Plan { ops, ..Plan::default() });
        assert_eq!(sets.iter().map(|(_, keys)| keys.len()).collect::<Vec<_>>(), [500, 500, 1]);
    }
}
