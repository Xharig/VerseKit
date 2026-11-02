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

//! Rust side of the parity check for the unit storage
//! (`tools/parity_orgstock.py`): the cases of the Python output through
//! `versekit_core::basetool::orgstock`, every request answered by a made-up
//! transport from the same table, nothing leaves the process.
//!
//! `--gegenprobe` hands out the lots sorted by name instead of in the
//! order of the feed.

use std::sync::{Arc, Mutex};

use versekit_core::basetool::config::Config;
use versekit_core::basetool::connection::{Connection, system_clock};
use versekit_core::basetool::error::ApiError;
use versekit_core::basetool::http::{Request, Response, Transport};
use versekit_core::basetool::key::StoredKey;
use versekit_core::basetool::orgstock::{self, Earmarks, Entry, Found, Lot, Member, OrgStock, OrgStockData, Resolved};
use versekit_core::basetool::secrets::{MemorySecrets, REFRESH_SECRET, SecretStore};
use versekit_core::jsonio::{self, Map, Value};

const BASE: &str = "https://parity.invalid";

fn obj(pairs: Vec<(&str, Value)>) -> Value {
    Value::Object(pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
}

fn decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => out.push(b' '),
            b'%' if i + 2 < bytes.len() => {
                match std::str::from_utf8(&bytes[i + 1..i + 3]).ok().and_then(|h| u8::from_str_radix(h, 16).ok()) {
                    Some(b) => {
                        out.push(b);
                        i += 2;
                    }
                    None => out.push(b'%'),
                }
            }
            other => out.push(other),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Answers from a table `path?sorted query` → answer or `{"error": code}`.
struct Table {
    answers: Map,
    asked: Mutex<Vec<String>>,
    /// Answers to `POST /catalog/resolve`, in order, and the bodies that came.
    script: Mutex<Vec<Value>>,
    bodies: Mutex<Vec<Value>>,
}

impl Transport for Table {
    fn send(&self, request: &Request) -> Result<Response, ApiError> {
        let reply = |status: u16, body: Value| Response {
            status,
            headers: Vec::new(),
            body: jsonio::dumps(&body, None, false).into_bytes(),
        };
        let (path, query) = request.url.split_once('?').unwrap_or((request.url.as_str(), ""));
        if path.ends_with("/.well-known/openid-configuration") {
            return Ok(reply(
                200,
                obj(vec![
                    ("issuer", Value::str(format!("{BASE}/auth"))),
                    ("device_authorization_endpoint", Value::str(format!("{BASE}/auth/device"))),
                    ("token_endpoint", Value::str(format!("{BASE}/auth/token"))),
                ]),
            ));
        }
        if path.ends_with("/auth/token") {
            return Ok(reply(
                200,
                obj(vec![
                    ("token_type", Value::str("DPoP")),
                    ("access_token", Value::str("a")),
                    ("expires_in", Value::int(300)),
                ]),
            ));
        }
        let route = path.strip_prefix(&format!("{BASE}/api")).unwrap_or(path);
        if request.method == "POST" && route == orgstock::RESOLVE {
            let body = request
                .body
                .as_ref()
                .and_then(|b| std::str::from_utf8(b).ok())
                .and_then(|t| jsonio::loads(t).ok())
                .unwrap_or(Value::Null);
            self.bodies.lock().map_err(|_| ApiError::network("lock"))?.push(body);
            let mut script = self.script.lock().map_err(|_| ApiError::network("lock"))?;
            if script.is_empty() {
                return Err(ApiError::network("missing"));
            }
            return Ok(reply(200, script.remove(0)));
        }
        let mut pairs: Vec<(String, String)> =
            query.split('&').filter_map(|p| p.split_once('=')).map(|(k, v)| (decode(k), decode(v))).collect();
        pairs.sort();
        let key = format!("{route}?{}", pairs.iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>().join("&"));
        self.asked.lock().map_err(|_| ApiError::network("lock"))?.push(format!("{} {key}", request.method));
        match self.answers.iter().find(|(k, _)| *k == key).map(|(_, v)| v) {
            None => Err(ApiError::network("missing")),
            Some(answer) => match answer.get("error").and_then(Value::as_str) {
                Some(code) => Ok(reply(403, obj(vec![("code", Value::str(code))]))),
                None => Ok(reply(200, answer.clone())),
            },
        }
    }
}

fn connection(answers: Map) -> (Connection, Arc<Table>) {
    let table = Arc::new(Table {
        answers,
        asked: Mutex::new(Vec::new()),
        script: Mutex::new(Vec::new()),
        bodies: Mutex::new(Vec::new()),
    });
    let config = Config {
        issuer: format!("{BASE}/auth"),
        api: format!("{BASE}/api"),
        client_id: "parity".into(),
        ca: String::new(),
        sandbox: true,
        overridden: true,
    };
    let secrets = Arc::new(MemorySecrets::default());
    let _ = secrets.save(REFRESH_SECRET, "r");
    let vault = Arc::new(StoredKey { secrets: secrets.clone() });
    (Connection::new(config, table.clone(), secrets, vault, "5.0.0", system_clock()), table)
}

fn float(x: f64) -> Value {
    Value::Float(x)
}

fn entry_json(e: &Entry) -> Value {
    obj(vec![
        ("id", Value::str(&e.id)),
        ("version", Value::int(e.version)),
        ("withheld", Value::Bool(e.withheld)),
        ("holder", Value::str(&e.holder)),
        ("ref", Value::str(&e.holder_ref)),
        ("amount", float(e.amount)),
        ("unit", Value::str(&e.unit)),
        ("note", Value::str(&e.note)),
        (
            "jobs",
            Value::Array(
                e.jobs
                    .iter()
                    .map(|j| {
                        obj(vec![
                            ("id", Value::str(&j.id)),
                            ("number", j.number.map_or(Value::Null, Value::int)),
                            ("amount", float(j.amount)),
                            ("delivered", Value::Bool(j.delivered)),
                        ])
                    })
                    .collect(),
            ),
        ),
        (
            "missions",
            Value::Array(
                e.missions
                    .iter()
                    .map(|m| {
                        obj(vec![("id", Value::str(&m.id)), ("name", Value::str(&m.name)), ("amount", float(m.amount))])
                    })
                    .collect(),
            ),
        ),
        ("free_jobs", float(e.free_jobs)),
        ("free_missions", float(e.free_missions)),
    ])
}

fn lot_json(l: &Lot) -> Value {
    let mut material = Vec::new();
    if !l.material_bt.is_empty() {
        material.push(("bt", Value::str(&l.material_bt)));
    }
    if !l.material_name.is_empty() {
        material.push(("name", Value::str(&l.material_name)));
    }
    obj(vec![
        ("key", Value::str(&l.key)),
        ("unit", obj(vec![("id", Value::str(&l.unit_id)), ("name", Value::str(&l.unit_name))])),
        ("material", obj(material)),
        ("name", Value::str(&l.name)),
        ("kind", l.kind.clone().unwrap_or(Value::Null)),
        ("location", Value::Object(l.location.clone())),
        ("place", Value::str(&l.place)),
        ("quality", Value::int(l.quality)),
        ("stolen", Value::Bool(l.stolen)),
        ("amount", float(l.amount)),
        ("unit_q", Value::str(&l.unit_q)),
        ("job", float(l.job)),
        ("mission", float(l.mission)),
        ("entries", l.entries.as_ref().map_or(Value::Null, |e| Value::Array(e.iter().map(entry_json).collect()))),
    ])
}

fn state_json(data: Option<&OrgStockData>) -> Value {
    let Some(data) = data else { return Value::Null };
    let units = data
        .units
        .iter()
        .map(|u| {
            obj(vec![
                ("id", Value::str(&u.id)),
                ("name", Value::str(&u.name)),
                ("shorthand", Value::str(&u.shorthand)),
                ("kind", Value::str(&u.kind)),
                ("logistician", u.logistician.map_or(Value::Null, Value::Bool)),
            ])
        })
        .collect();
    let rosters = data
        .rosters
        .iter()
        .map(|(id, members)| {
            let list = members
                .iter()
                .map(|m| obj(vec![("ref", Value::str(&m.holder_ref)), ("name", Value::str(&m.name))]))
                .collect();
            (id.clone(), Value::Array(list))
        })
        .collect();
    obj(vec![
        ("lots", Value::Array(data.lots.iter().map(lot_json).collect())),
        ("units", Value::Array(units)),
        ("logistician", Value::Array(data.logistician.iter().map(Value::str).collect())),
        ("rosters", Value::Object(rosters)),
        ("details", Value::Bool(data.details)),
        ("write", Value::Bool(data.write)),
    ])
}

fn field<'a>(v: &'a Value, key: &str) -> Result<&'a Value, String> {
    v.get(key).ok_or_else(|| format!("{key} fehlt"))
}

fn array<'a>(v: &'a Value, key: &str) -> Result<&'a [Value], String> {
    match field(v, key)? {
        Value::Array(items) => Ok(items),
        _ => Err(format!("{key} ist keine Liste")),
    }
}

fn number(v: &Value) -> f64 {
    match v {
        Value::Int(n) => n.parse().unwrap_or(0.0),
        Value::Float(x) => *x,
        _ => 0.0,
    }
}

fn text(v: &Value) -> String {
    v.as_str().unwrap_or_default().to_string()
}

fn earmarks(v: &Value) -> Earmarks {
    v.as_object().map(|m| m.iter().map(|(k, a)| (k.clone(), number(a))).collect()).unwrap_or_default()
}

fn entry_of(v: &Value) -> Entry {
    let base = orgstock::parse_entry(&obj(vec![("id", Value::str(text(field(v, "id").unwrap_or(&Value::Null))))]));
    let mut entry = base.unwrap_or_else(|| orgstock::parse_entry(&obj(vec![("id", Value::str("?"))])).expect("entry"));
    entry.version = v.get("version").map_or(0, |n| number(n) as i64);
    entry
}

fn op(name: &str, a: &[Value]) -> Result<Value, String> {
    let s = |i: usize| text(&a[i]);
    let f = |i: usize| number(&a[i]);
    let b = |i: usize| matches!(a[i], Value::Bool(true));
    Ok(match name {
        "book_in" => {
            orgstock::op_book_in(&s(0), &s(1), a[2].clone(), a[3].clone(), f(4) as i64, b(5), f(6), &s(7), &s(8), b(9))
        }
        "book_out" => orgstock::op_book_out(&s(0), &entry_of(&a[1]), f(2), &s(3), &earmarks(&a[4]), &earmarks(&a[5])),
        "sell" => {
            orgstock::op_sell(&s(0), &entry_of(&a[1]), f(2), &s(3), &s(4), f(5), &earmarks(&a[6]), &earmarks(&a[7]))
        }
        "move_location" => orgstock::op_move_location(
            &s(0),
            &entry_of(&a[1]),
            f(2),
            &s(3),
            a[4].clone(),
            &earmarks(&a[5]),
            &earmarks(&a[6]),
            b(7),
        ),
        "move_holder" => orgstock::op_move_holder(
            &s(0),
            &entry_of(&a[1]),
            f(2),
            &s(3),
            &s(4),
            &s(5),
            &earmarks(&a[6]),
            &earmarks(&a[7]),
            b(8),
        ),
        "move_pool" => {
            let amount = (!matches!(a[3], Value::Null)).then(|| f(3));
            orgstock::op_move_pool(&s(0), &entry_of(&a[1]), &s(2), amount, &s(4), b(5))
        }
        "note" => orgstock::op_note(&s(0), &entry_of(&a[1]), &s(2)),
        "stolen" => {
            let amount = (!matches!(a[3], Value::Null)).then(|| f(3));
            orgstock::op_stolen(&s(0), &entry_of(&a[1]), b(2), amount, &s(4))
        }
        "allocation" => orgstock::op_allocation(&s(0), &s(1), &entry_of(&a[2]), &s(3), &s(4), f(5), &s(6)),
        other => return Err(format!("unbekannte Anweisung {other}")),
    })
}

fn found_json(f: &Found) -> Value {
    obj(vec![("bt", Value::str(&f.bt)), ("name", Value::str(&f.name)), ("kind", Value::str(&f.kind))])
}

pub fn run(python_json: &str, out: &str, gegenprobe: bool) -> Result<(), String> {
    let input = jsonio::read_json(python_json).map_err(|e| e.to_string())?;
    let cases = field(&input, "cases")?;

    let mut refreshes = Vec::new();
    for scenario in array(cases, "scenarios")? {
        let store = OrgStock::new();
        let mut steps = Vec::new();
        for (i, step) in array(scenario, "steps")?.iter().enumerate() {
            let answers = field(step, "answers")?.as_object().cloned().unwrap_or_default();
            let (conn, table) = connection(answers);
            let now = 1000.0 + i as f64;
            let details = matches!(field(step, "details")?, Value::Bool(true));
            let write = matches!(field(step, "write")?, Value::Bool(true));
            let (counts, error) = match store.refresh(&conn, details, write, now) {
                Ok(counts) => (Value::Object(counts), Value::Null),
                Err(e) => (Value::Null, Value::str(e.code)),
            };
            let asked = table.asked.lock().map_err(|_| "lock")?.iter().map(Value::str).collect();
            let (_, data) = store.current(now);
            let mut state = state_json(data.as_deref());
            if gegenprobe && let Some(data) = data.as_deref() {
                let mut sorted = data.clone();
                sorted.lots.sort_by(|x, y| x.name.cmp(&y.name));
                state = state_json(Some(&sorted));
            }
            steps.push(obj(vec![
                ("counts", counts),
                ("error", error),
                ("asked", Value::Array(asked)),
                ("state", state),
            ]));
        }
        refreshes.push(obj(vec![("name", field(scenario, "name")?.clone()), ("steps", Value::Array(steps))]));
    }

    // Lists on one fetched state.
    let snapshot = field(cases, "snapshot")?.clone();
    let roster = field(cases, "roster")?.clone();
    let answers: Map = vec![
        ("/org-stock?limit=500".into(), snapshot),
        ("/org-stock/members?unit=u1".into(), roster.clone()),
        ("/org-stock/members?unit=u2".into(), roster),
    ];
    let (conn, _) = connection(answers);
    let store = OrgStock::new();
    store.refresh(&conn, true, true, 1000.0).map_err(|e| e.to_string())?;
    let data = store.current(1000.0).1.ok_or("kein Stand")?;
    let mut lots_of = Vec::new();
    for unit in ["", "u1", "u2", "u9"] {
        for search in array(cases, "searches")? {
            let search = text(search);
            let keys = data.lots_of(unit, &search).iter().map(|l| Value::str(&l.key)).collect();
            lots_of.push(Value::Array(vec![Value::str(unit), Value::str(search), Value::Array(keys)]));
        }
    }
    let mut known = Vec::new();
    for name in ["gold", " Laranite ", "FR-66", "nur-bt", "X"] {
        for unit in ["SCU", "PIECE"] {
            let hit = data
                .known_material(name, unit)
                .map_or(Value::Null, |f| obj(vec![("bt", Value::str(f.bt)), ("name", Value::str(f.name))]));
            known.push(Value::Array(vec![Value::str(name), Value::str(unit), hit]));
        }
    }
    let can_write = ["u1", "u2", "u3", "u9"]
        .iter()
        .map(|u| Value::Array(vec![Value::str(*u), Value::Bool(data.can_write(u))]))
        .collect();
    let lists = obj(vec![
        ("lots_of", Value::Array(lots_of)),
        ("known", Value::Array(known)),
        ("can_write", Value::Array(can_write)),
    ]);

    let mut ops = Vec::new();
    for case in array(cases, "ops")? {
        let Value::Array(parts) = case else { return Err("Anweisung ist keine Liste".into()) };
        let Value::Array(args) = &parts[1] else { return Err("Argumente fehlen".into()) };
        ops.push(op(&text(&parts[0]), args)?);
    }

    let results = array(cases, "results")?
        .iter()
        .map(|r| {
            let outcome = orgstock::read_result(&r.as_object().cloned().unwrap_or_default());
            let failed = outcome
                .failed
                .iter()
                .map(|(index, kind, reason)| Value::Array(vec![index.clone(), Value::str(kind), Value::str(reason)]))
                .collect();
            obj(vec![
                ("applied", Value::int(outcome.applied)),
                ("unchanged", Value::int(outcome.unchanged)),
                ("failed", Value::Array(failed)),
                ("offers", Value::int(outcome.offers)),
                ("reasons", Value::Array(outcome.reasons().into_iter().map(Value::Str).collect())),
            ])
        })
        .collect();

    let mut lookups = Vec::new();
    for case in array(cases, "lookups")? {
        // Through the real request: the body the Basetool gets is compared.
        let (conn, table) = connection(Map::new());
        *table.script.lock().map_err(|_| "lock")? = array(case, "answers")?.to_vec();
        let known = match field(case, "known")? {
            Value::Array(k) if k.len() == 2 => Some((text(&k[0]), k[1].clone())),
            _ => None,
        };
        let piece = matches!(field(case, "piece")?, Value::Bool(true));
        let name = text(field(case, "name")?);
        let result = orgstock::resolve_material(&conn, &name, known.as_ref().map(|(k, r)| (k.as_str(), r)), piece);
        let asked = table.bodies.lock().map_err(|_| "lock")?.clone();
        let outcome = match result {
            Ok(Resolved::One(f)) => vec![Value::str("resolved"), found_json(&f)],
            Ok(Resolved::Several(hits)) => {
                vec![Value::str("ambiguous"), Value::Array(hits.iter().map(found_json).collect())]
            }
            Ok(Resolved::Unmatched) => vec![Value::str("unmatched"), Value::Null],
            Err(e) => vec![Value::str("error"), Value::str(e.code)],
        };
        lookups.push(obj(vec![("outcome", Value::Array(outcome)), ("asked", Value::Array(asked))]));
    }

    let numbers = array(cases, "numbers")?
        .iter()
        .map(|raw| Value::Array(vec![raw.clone(), orgstock::number(&text(raw)).map_or(Value::Null, Value::Float)]))
        .collect();
    let views = array(cases, "views")?
        .iter()
        .map(|v| Value::Array(vec![v.clone(), Value::str(orgstock::view_of(v.as_str()))]))
        .collect();
    let own = array(cases, "rosters")?
        .iter()
        .map(|case| {
            let Value::Array(parts) = case else { return Value::Null };
            let roster: Vec<Member> = match &parts[0] {
                Value::Array(members) => members
                    .iter()
                    .map(|m| Member {
                        holder_ref: text(m.get("ref").unwrap_or(&Value::Null)),
                        name: text(m.get("name").unwrap_or(&Value::Null)),
                    })
                    .collect(),
                _ => Vec::new(),
            };
            Value::str(orgstock::own_member(&roster, parts[1].as_str()))
        })
        .collect();
    let quantities = array(cases, "quantities")?
        .iter()
        .map(|q| match q {
            Value::Array(p) => orgstock::quantity(number(&p[0]), &text(&p[1])),
            _ => Value::Null,
        })
        .collect();
    let mut reasons: Vec<&str> = orgstock::REASONS.to_vec();
    reasons.sort_unstable();
    let helpers = obj(vec![
        ("number", Value::Array(numbers)),
        ("view_of", Value::Array(views)),
        ("own_member", Value::Array(own)),
        ("quantity", Value::Array(quantities)),
        ("reasons", Value::Array(reasons.into_iter().map(Value::str).collect())),
    ]);

    let result = obj(vec![
        ("refreshes", Value::Array(refreshes)),
        ("lists", lists),
        ("ops", Value::Array(ops)),
        ("results", Value::Array(results)),
        ("lookups", Value::Array(lookups)),
        ("helpers", helpers),
    ]);
    std::fs::write(out, jsonio::dumps(&result, Some(1), false)).map_err(|e| e.to_string())?;
    println!("geschrieben: {out}");
    Ok(())
}
