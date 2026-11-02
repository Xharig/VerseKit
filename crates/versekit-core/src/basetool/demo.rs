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

//! A made-up Basetool in memory, for the preview and the tests: the unit
//! storage with three units, members, orders, missions, goods, locations,
//! and bookings that change it. Nothing leaves the process: it answers as a
//! [`Transport`], with its own login (discovery and token) and a key and
//! refresh token in memory only.
//!
//! Every feed request after the first changes one entry as another member
//! would, so a booking can meet `VERSION_CONFLICT` and the page shows a
//! change from outside.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use crate::basetool::config::Config;
use crate::basetool::connection::{Clock, Connection, get, get_str};
use crate::basetool::demand::{DemandArea, DemandStore};
use crate::basetool::error::{ApiError, truthy};
use crate::basetool::http::{Request, Response, Transport};
use crate::basetool::key::StoredKey;
use crate::basetool::orgstock::{OrgStock, OrgStockArea, PIECE, SCU};
use crate::basetool::orgunits::{OrgUnit, OrgUnitsArea};
use crate::basetool::secrets::{MemorySecrets, REFRESH_SECRET, SecretStore};
use crate::basetool::sync::{Setup, Shared, SyncHandle, Waker};
use crate::jsonio::{self, Map, Value};
use crate::paths::{Host, Paths};
use crate::stock_data::lower;
use crate::writeguard;

pub const ISSUER: &str = "https://beispiel.profit-base.invalid/auth/realms/iri";
pub const API: &str = "https://beispiel.profit-base.invalid/exchange/v1";
/// A booking of more than this many SCU asks for a confirmation in the browser.
pub const MASS_LIMIT: f64 = 5000.0;

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

#[derive(Clone, Debug)]
struct DemoEntry {
    id: String,
    version: i64,
    holder: String,
    withheld: bool,
    note: String,
    amount: f64,
    jobs: Vec<(String, i64, f64, bool)>,
    missions: Vec<(String, String, f64)>,
}

#[derive(Clone, Debug)]
struct DemoLot {
    key: String,
    unit: String,
    bt: String,
    place: String,
    quality: i64,
    stolen: bool,
    entries: Vec<DemoEntry>,
}

/// One kind of goods: id, name, piece or SCU, material kind (none for an
/// item), component.
struct Goods {
    bt: &'static str,
    name: &'static str,
    piece: bool,
    kind: Option<&'static str>,
    component: Option<&'static str>,
}

const GOODS: [Goods; 9] = [
    Goods { bt: "bt-laranite", name: "Laranite", piece: false, kind: Some("REFINED"), component: None },
    Goods { bt: "bt-quantainium", name: "Quantainium", piece: false, kind: Some("REFINED"), component: None },
    Goods { bt: "bt-agricium-ore", name: "Agricium (Ore)", piece: false, kind: Some("RAW"), component: None },
    Goods { bt: "bt-agricium", name: "Agricium", piece: false, kind: Some("REFINED"), component: None },
    Goods { bt: "bt-medical", name: "Medical Supplies", piece: false, kind: Some("NO_REFINE"), component: None },
    Goods { bt: "bt-hadanite", name: "Hadanite", piece: true, kind: Some("RAW"), component: None },
    Goods {
        bt: "bt-fr66",
        name: "FR-66",
        piece: true,
        kind: None,
        component: Some(r#"{"componentType": "SHIELD", "size": 2, "grade": "C", "itemClass": "MILITARY"}"#),
    },
    Goods {
        bt: "bt-bulwark",
        name: "Bulwark",
        piece: true,
        kind: None,
        component: Some(r#"{"componentType": "COOLER", "size": 1, "grade": "B", "itemClass": "CIVILIAN"}"#),
    },
    Goods { bt: "bt-pembroke", name: "Pembroke Helm", piece: true, kind: None, component: None },
];

const PLACES: [&str; 6] = ["Area18", "Lorville", "New Babbage", "Orison", "Seraphim Station", "Port Tressler"];

/// Units: id, name, shorthand, kind, the member is logistician there.
const UNITS: [(&str, &str, &str, &str, bool); 3] = [
    ("u-alpha", "Staffel Alpha", "ALP", "SQUADRON", true),
    ("u-fracht", "Frachtkommando", "FRK", "SPECIAL_COMMAND", true),
    ("u-industrie", "Bereich Industrie", "IND", "BEREICH", false),
];

/// Members per unit: ref and name.
const MEMBERS: [(&str, &[(&str, &str)]); 2] = [
    ("u-alpha", &[("r-kestrel", "Kestrel"), ("r-nova", "Nova"), ("r-player", "PlayerOne"), ("r-vega", "Vega")]),
    ("u-fracht", &[("r-nova", "Nova"), ("r-orbit", "Orbit"), ("r-player", "PlayerOne")]),
];

/// Orders: id, number, status, goods.
const ORDERS: [(&str, i64, &str, &str); 3] =
    [("o-12", 12, "OPEN", "bt-laranite"), ("o-15", 15, "IN_PROGRESS", "bt-laranite"), ("o-17", 17, "OPEN", "bt-fr66")];
const MISSIONS: [(&str, &str, &str); 2] = [("ms-1", "Konvoi Pyro", "PLANNED"), ("ms-2", "Bergbau Daymar", "ACTIVE")];

/// How the made-up Basetool answers a device login.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DemoLogin {
    /// Pending for the first questions, then granted.
    Approve,
    /// The member refuses in the browser.
    Deny,
    /// The code runs out.
    Expire,
}

/// The rights of a connection that is made without a login (the preview and
/// the tests that start connected).
const DEFAULT_RIGHTS: [&str; 4] =
    ["exchange.orgstock.read", "exchange.orgstock.details", "exchange.orgstock.write", "exchange.demand.read"];

/// The code the made-up Basetool shows for a login.
pub const USER_CODE: &str = "DEMO-4711";
const DEVICE_CODE: &str = "demo-device";

struct World {
    lots: Vec<DemoLot>,
    /// (revision, key) of every change, for the feed.
    history: Vec<(u64, String)>,
    revision: u64,
    next: u64,
    feeds: u64,
    /// Method and path with query of every request, for the tests.
    seen: Vec<String>,
    /// Every change set that came in.
    bodies: Vec<Value>,
    /// The rights of the last login; none before the first one.
    granted: Option<Vec<String>>,
    /// The rights asked for by the login that runs.
    asked: Vec<String>,
    polls: u32,
    /// The question that grants the login.
    approve_at: u32,
    login: DemoLogin,
    /// What the account check answers.
    account: String,
    /// The service document fails with this status and code.
    service_error: Option<(u16, String)>,
}

/// The made-up Basetool.
pub struct DemoBasetool {
    world: Mutex<World>,
}

fn entry(id: &str, holder: &str, amount: f64) -> DemoEntry {
    DemoEntry {
        id: id.into(),
        version: 1,
        holder: holder.into(),
        withheld: false,
        note: String::new(),
        amount,
        jobs: Vec::new(),
        missions: Vec::new(),
    }
}

fn lot(key: &str, unit: &str, bt: &str, place: &str, quality: i64, stolen: bool, entries: Vec<DemoEntry>) -> DemoLot {
    DemoLot { key: key.into(), unit: unit.into(), bt: bt.into(), place: place.into(), quality, stolen, entries }
}

impl Default for DemoBasetool {
    fn default() -> Self {
        DemoBasetool::new()
    }
}

impl DemoBasetool {
    pub fn new() -> DemoBasetool {
        let mut laranite = entry("e-1", "r-nova", 40.0);
        laranite.note = "Kiste am Hangar 3".into();
        laranite.jobs.push(("o-12".into(), 12, 10.0, false));
        let mut laranite_two = entry("e-2", "r-kestrel", 12.5);
        laranite_two.missions.push(("ms-1".into(), "Konvoi Pyro".into(), 2.5));
        let mut hidden = entry("e-3", "", 6.0);
        hidden.withheld = true;
        let lots = vec![
            lot("L-1", "u-alpha", "bt-laranite", "Area18", 612, false, vec![laranite, laranite_two, hidden]),
            lot("L-2", "u-alpha", "bt-quantainium", "Lorville", 880, false, vec![entry("e-4", "r-player", 32.0)]),
            lot("L-3", "u-alpha", "bt-agricium-ore", "Orison", 401, true, vec![entry("e-5", "r-vega", 8.75)]),
            lot("L-4", "u-alpha", "bt-hadanite", "Area18", 950, false, vec![entry("e-6", "r-nova", 3.0)]),
            lot("L-5", "u-alpha", "bt-fr66", "New Babbage", 0, false, vec![entry("e-7", "r-player", 2.0)]),
            lot("L-6", "u-alpha", "bt-bulwark", "Lorville", 0, false, vec![entry("e-8", "r-kestrel", 4.0)]),
            lot("L-7", "u-alpha", "bt-pembroke", "Area18", 0, false, vec![entry("e-9", "r-vega", 1.0)]),
            lot("L-8", "u-fracht", "bt-medical", "Port Tressler", 0, false, vec![entry("e-10", "r-orbit", 120.0)]),
            lot("L-9", "u-industrie", "bt-agricium", "Seraphim Station", 520, false, vec![entry("e-11", "r-x", 64.0)]),
        ];
        DemoBasetool {
            world: Mutex::new(World {
                lots,
                history: Vec::new(),
                revision: 1,
                next: 100,
                feeds: 0,
                seen: Vec::new(),
                bodies: Vec::new(),
                granted: None,
                asked: Vec::new(),
                polls: 0,
                approve_at: 2,
                login: DemoLogin::Approve,
                account: "match".into(),
                service_error: None,
            }),
        }
    }

    /// The connection to it: a key and a refresh token in memory, nothing in
    /// the system's store.
    pub fn connection(self: &Arc<Self>, clock: Clock) -> Connection {
        self.connection_with(clock, true)
    }

    /// The same connection before anyone logged in: it has no token yet, and
    /// the device login of the made-up Basetool makes one.
    pub fn connection_unlinked(self: &Arc<Self>, clock: Clock) -> Connection {
        self.connection_with(clock, false)
    }

    fn connection_with(self: &Arc<Self>, clock: Clock, linked: bool) -> Connection {
        let config = Config {
            issuer: ISSUER.into(),
            api: API.into(),
            client_id: "versekit".into(),
            ca: String::new(),
            sandbox: true,
            overridden: true,
        };
        let secrets = Arc::new(MemorySecrets::default());
        if linked {
            let _ = secrets.save(REFRESH_SECRET, "demo-refresh");
        }
        let vault = Arc::new(StoredKey { secrets: secrets.clone() });
        Connection::new(config, self.clone(), secrets, vault, "5.0.0", clock)
    }

    /// How the next logins end, and at which question they are granted
    /// (1 is the first question).
    pub fn script_login(&self, outcome: DemoLogin, approve_at: u32) {
        let mut world = lock(&self.world);
        world.login = outcome;
        world.approve_at = approve_at;
    }

    /// What the account check answers (`match`, `mismatch`, `unknown`).
    pub fn script_account(&self, answer: &str) {
        lock(&self.world).account = answer.to_string();
    }

    /// The service document fails with `status` and `code` until `None`.
    pub fn script_service_error(&self, error: Option<(u16, &str)>) {
        lock(&self.world).service_error = error.map(|(status, code)| (status, code.to_string()));
    }

    /// The rights the last login granted.
    pub fn granted(&self) -> Option<Vec<String>> {
        lock(&self.world).granted.clone()
    }

    /// Method and path with query of every request so far.
    pub fn seen(&self) -> Vec<String> {
        lock(&self.world).seen.clone()
    }

    /// Every change set so far.
    pub fn change_sets(&self) -> Vec<Value> {
        lock(&self.world).bodies.clone()
    }
}

/// The made-up Basetool with a sync of its own: settings in a throwaway
/// folder, the areas that exist, and a connection that starts unlinked.
/// What the preview and the tests of the pages stand on.
pub struct DemoSync {
    pub demo: Arc<DemoBasetool>,
    pub sync: SyncHandle,
    pub org_stock: Arc<OrgStock>,
    pub demand: Arc<DemandStore>,
    pub units: Shared<Vec<OrgUnit>>,
    pub paths: Arc<Paths>,
}

impl DemoSync {
    /// `folder` holds the settings and the state files of the made-up
    /// Basetool, nothing else; `worker` starts the sync in the background
    /// (without it, passes run only through `SyncHandle::pass_now`).
    /// ⚠ The sync is made with the write lock off (the settings of the pages
    /// reach `folder` through it); only `folder` is ever written.
    pub fn new(folder: &str, waker: Waker, own_account: Option<String>, worker: bool) -> DemoSync {
        let mut host = Host::current();
        host.set_var(crate::legacy::ENV_HOME, folder);
        let paths = Arc::new(Paths::new(host));
        let demo = Arc::new(DemoBasetool::new());
        let clock = crate::basetool::connection::system_clock();
        let org_stock = OrgStock::new();
        let demand = DemandStore::new();
        let units = OrgUnitsArea::default();
        let unit_list = units.units.clone();
        let own_before = writeguard::state();
        writeguard::deny(false);
        let setup = Setup {
            conn: Arc::new(demo.connection_unlinked(clock.clone())),
            paths: paths.clone(),
            clock,
            version: "5.0.0".into(),
            own_account: Arc::new(move || own_account.clone()),
            waker,
            areas: vec![
                Box::new(units),
                Box::new(DemandArea { store: demand.clone() }),
                Box::new(OrgStockArea { store: org_stock.clone() }),
            ],
        };
        let sync = if worker { SyncHandle::start(setup) } else { SyncHandle::without_worker(setup) };
        writeguard::adopt(&own_before);
        DemoSync { demo, sync, org_stock, demand, units: unit_list, paths }
    }
}

fn goods(bt: &str) -> Option<&'static Goods> {
    GOODS.iter().find(|g| g.bt == bt)
}

fn amount_value(amount: f64, piece: bool) -> Value {
    let rounded = (amount * 1000.0).round() / 1000.0;
    Value::Object(vec![
        ("amount".into(), if piece { Value::int(rounded.round() as i64) } else { Value::Float(rounded) }),
        ("unit".into(), Value::str(if piece { PIECE } else { SCU })),
    ])
}

fn decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => out.push(b' '),
            b'%' if i + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok().and_then(|h| u8::from_str_radix(h, 16).ok());
                match hex {
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

fn query_of(query: &str) -> Vec<(String, String)> {
    query.split('&').filter_map(|pair| pair.split_once('=')).map(|(k, v)| (decode(k), decode(v))).collect()
}

fn answer(status: u16, body: Value) -> Response {
    Response { status, headers: Vec::new(), body: jsonio::dumps(&body, None, false).into_bytes() }
}

fn problem(status: u16, code: &str) -> Response {
    answer(status, Value::Object(vec![("code".into(), Value::str(code))]))
}

fn object(pairs: Vec<(&str, Value)>) -> Value {
    Value::Object(pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
}

fn number_of(value: Option<&Value>) -> f64 {
    match value {
        Some(Value::Int(n)) => n.parse().unwrap_or(0.0),
        Some(Value::Float(x)) => *x,
        _ => 0.0,
    }
}

fn quantity_of(op: &Map) -> Option<f64> {
    get(op, "quantity").map(|q| number_of(q.get("amount")))
}

impl World {
    fn touch(&mut self, key: &str) {
        self.revision += 1;
        self.history.push((self.revision, key.to_string()));
    }

    fn fresh_id(&mut self, prefix: &str) -> String {
        self.next += 1;
        format!("{prefix}-{}", self.next)
    }

    fn unit_json(&self) -> Value {
        Value::Array(
            UNITS
                .iter()
                .map(|(id, name, short, kind, logistician)| {
                    object(vec![
                        ("id", Value::str(*id)),
                        ("name", Value::str(*name)),
                        ("shorthand", Value::str(*short)),
                        ("kind", Value::str(*kind)),
                        ("logistician", Value::Bool(*logistician)),
                    ])
                })
                .collect(),
        )
    }

    fn holder_name(unit: &str, holder: &str) -> String {
        MEMBERS
            .iter()
            .filter(|(u, _)| *u == unit || unit.is_empty())
            .flat_map(|(_, list)| list.iter())
            .chain(MEMBERS.iter().flat_map(|(_, list)| list.iter()))
            .find(|(r, _)| *r == holder)
            .map_or_else(|| "Jemand".to_string(), |(_, n)| (*n).to_string())
    }

    fn lot_json(&self, lot: &DemoLot) -> Value {
        let goods = goods(&lot.bt);
        let piece = goods.is_some_and(|g| g.piece);
        let unit = UNITS.iter().find(|u| u.0 == lot.unit);
        let total: f64 = lot.entries.iter().map(|e| e.amount).sum();
        let jobs: f64 = lot.entries.iter().flat_map(|e| e.jobs.iter()).map(|j| j.2).sum();
        let missions: f64 = lot.entries.iter().flat_map(|e| e.missions.iter()).map(|m| m.2).sum();
        let mut pairs = vec![
            ("key", Value::str(&lot.key)),
            ("unit", object(vec![("id", Value::str(&lot.unit)), ("name", Value::str(unit.map_or("", |u| u.1)))])),
            (
                "material",
                object(vec![("bt", Value::str(&lot.bt)), ("name", Value::str(goods.map_or("?", |g| g.name)))]),
            ),
        ];
        if let Some(kind) = goods.and_then(|g| g.kind) {
            pairs.push(("materialKind", object(vec![("type", Value::str(kind)), ("commodity", Value::Bool(true))])));
        }
        if let Some(component) = goods.and_then(|g| g.component).and_then(|c| jsonio::loads(c).ok()) {
            pairs.push(("component", component));
        }
        pairs.extend([
            ("location", object(vec![("name", Value::str(&lot.place))])),
            ("quality", Value::int(lot.quality)),
            ("stolen", Value::Bool(lot.stolen)),
            ("quantity", amount_value(total, piece)),
            (
                "earmarked",
                object(vec![("jobOrders", amount_value(jobs, piece)), ("missions", amount_value(missions, piece))]),
            ),
        ]);
        if unit.is_some_and(|u| u.4) {
            let entries = lot
                .entries
                .iter()
                .map(|e| {
                    let holder = if e.withheld {
                        object(vec![("withheld", Value::Bool(true))])
                    } else {
                        object(vec![
                            ("withheld", Value::Bool(false)),
                            ("name", Value::str(World::holder_name(&lot.unit, &e.holder))),
                            ("ref", Value::str(&e.holder)),
                        ])
                    };
                    let job_sum: f64 = e.jobs.iter().map(|j| j.2).sum();
                    let mission_sum: f64 = e.missions.iter().map(|m| m.2).sum();
                    let mut fields = vec![
                        ("id", Value::str(&e.id)),
                        ("version", Value::int(e.version)),
                        ("holder", holder),
                        ("quantity", amount_value(e.amount, piece)),
                    ];
                    if !e.note.is_empty() && !e.withheld {
                        fields.push(("note", Value::str(&e.note)));
                    }
                    fields.extend([
                        (
                            "jobOrders",
                            Value::Array(
                                e.jobs
                                    .iter()
                                    .map(|(id, number, amount, delivered)| {
                                        object(vec![
                                            ("id", Value::str(id)),
                                            ("number", Value::int(*number)),
                                            ("quantity", amount_value(*amount, piece)),
                                            ("delivered", Value::Bool(*delivered)),
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
                                    .map(|(id, name, amount)| {
                                        object(vec![
                                            ("id", Value::str(id)),
                                            ("name", Value::str(name)),
                                            ("quantity", amount_value(*amount, piece)),
                                        ])
                                    })
                                    .collect(),
                            ),
                        ),
                        (
                            "free",
                            object(vec![
                                ("jobOrders", amount_value(e.amount - job_sum, piece)),
                                ("missions", amount_value(e.amount - mission_sum, piece)),
                            ]),
                        ),
                    ]);
                    object(fields)
                })
                .collect();
            pairs.push(("entries", Value::Array(entries)));
        }
        object(pairs)
    }

    fn page(&mut self, cursor: Option<&str>) -> Response {
        let since = match cursor {
            None => None,
            Some(c) => match c.strip_prefix('c').and_then(|n| n.parse::<u64>().ok()) {
                Some(n) if n <= self.revision => Some(n),
                _ => return problem(410, "CURSOR_EXPIRED"),
            },
        };
        if since.is_some() {
            self.feeds += 1;
            // Another member's change: Nova adds a little Laranite.
            if let Some(found) = self.lots.iter_mut().find(|l| l.key == "L-1")
                && let Some(e) = found.entries.iter_mut().find(|e| e.holder == "r-nova")
            {
                e.amount += 1.0;
                e.version += 1;
                self.touch("L-1");
            }
        }
        let (items, removed): (Vec<Value>, Vec<Value>) = match since {
            None => (self.lots.iter().map(|l| self.lot_json(l)).collect(), Vec::new()),
            Some(n) => {
                let mut keys: Vec<String> =
                    self.history.iter().filter(|(rev, _)| *rev > n).map(|(_, key)| key.clone()).collect();
                keys.dedup();
                let mut seen = Vec::new();
                let (mut items, mut removed) = (Vec::new(), Vec::new());
                for key in keys {
                    if seen.contains(&key) {
                        continue;
                    }
                    seen.push(key.clone());
                    match self.lots.iter().find(|l| l.key == key) {
                        Some(lot) => items.push(self.lot_json(lot)),
                        None => removed.push(object(vec![("key", Value::str(&key))])),
                    }
                }
                (items, removed)
            }
        };
        answer(
            200,
            object(vec![
                ("items", Value::Array(items)),
                ("removed", Value::Array(removed)),
                ("units", self.unit_json()),
                ("nextCursor", Value::str(format!("c{}", self.revision))),
                ("hasMore", Value::Bool(false)),
            ]),
        )
    }

    fn roster(unit: &str) -> Option<&'static [(&'static str, &'static str)]> {
        MEMBERS.iter().find(|(u, _)| *u == unit).map(|(_, list)| *list)
    }

    /// What the units still need; the single orders only for a login that
    /// was granted the right to see them.
    fn demand(&self) -> Response {
        let amount = |amount: f64, unit: &str| {
            object(vec![
                ("amount", if unit == PIECE { Value::int(amount as i64) } else { Value::Float(amount) }),
                ("unit", Value::str(unit)),
            ])
        };
        let named = |name: &str| object(vec![("name", Value::str(name))]);
        let material = |name: &str, open: Value, quality: i64, raw: &[&str]| {
            object(vec![
                ("material", named(name)),
                ("openQuantity", open),
                ("minQuality", Value::int(quality)),
                ("rawRefs", Value::Array(raw.iter().map(|r| named(r)).collect())),
                ("source", Value::str("ORDER")),
            ])
        };
        let item = |name: &str, count: i64, craftable: bool| {
            object(vec![
                ("item", named(name)),
                ("openQuantity", amount(count as f64, PIECE)),
                ("craftableByMe", Value::Bool(craftable)),
            ])
        };
        let mut body = vec![
            (
                "materials",
                Value::Array(vec![
                    material("Laranite", amount(18.5, SCU), 600, &["Laranite (Raw)"]),
                    material("Agricium", amount(40.0, SCU), 0, &["Agricium (Ore)"]),
                    material("Hadanite", amount(12.0, PIECE), 500, &[]),
                ]),
            ),
            ("items", Value::Array(vec![item("FR-66 Shield", 2, true), item("Bulwark Armor", 1, false)])),
            ("updatedAt", Value::str("2026-10-22T10:00:00Z")),
        ];
        let details = self.granted.as_ref().is_some_and(|g| g.iter().any(|r| r == "exchange.demand.details"));
        if details {
            let order = object(vec![
                ("id", Value::str("o-12")),
                ("number", Value::int(12)),
                ("type", Value::str("MATERIAL")),
                ("status", Value::str("OPEN")),
                ("priority", Value::int(1)),
                ("unit", named("Alpha")),
                ("requestingUnit", named("Fracht")),
                ("handle", Value::str("PlayerTwo")),
                ("comment", Value::str("Bitte bis Freitag")),
                ("materials", Value::Array(vec![material("Laranite", amount(10.0, SCU), 600, &[])])),
                ("items", Value::Array(vec![item("FR-66 Shield", 1, true)])),
            ]);
            body.push(("orders", Value::Array(vec![order])));
        }
        answer(200, object(body))
    }

    fn members(&self, unit: &str) -> Response {
        let Some(list) = World::roster(unit) else { return problem(403, "NOT_PERMITTED") };
        let items = list.iter().map(|(r, n)| object(vec![("ref", Value::str(*r)), ("name", Value::str(*n))])).collect();
        let name = UNITS.iter().find(|u| u.0 == unit).map_or("", |u| u.1);
        answer(
            200,
            object(vec![
                ("unit", object(vec![("id", Value::str(unit)), ("name", Value::str(name))])),
                ("items", Value::Array(items)),
            ]),
        )
    }

    fn targets(&self, unit: &str, material: &str) -> Response {
        if World::roster(unit).is_none() {
            return problem(403, "NOT_PERMITTED");
        }
        let piece = goods(material).is_some_and(|g| g.piece);
        let jobs = ORDERS
            .iter()
            .filter(|o| o.3 == material)
            .map(|(id, number, status, _)| {
                object(vec![("id", Value::str(*id)), ("number", Value::int(*number)), ("status", Value::str(*status))])
            })
            .collect();
        let missions = if piece {
            Vec::new()
        } else {
            MISSIONS
                .iter()
                .map(|(id, name, status)| {
                    object(vec![("id", Value::str(*id)), ("name", Value::str(*name)), ("status", Value::str(*status))])
                })
                .collect()
        };
        let name = UNITS.iter().find(|u| u.0 == unit).map_or("", |u| u.1);
        answer(
            200,
            object(vec![
                ("unit", object(vec![("id", Value::str(unit)), ("name", Value::str(name))])),
                ("jobOrders", Value::Array(jobs)),
                ("missions", Value::Array(missions)),
            ]),
        )
    }

    fn resolve(body: &Value) -> Response {
        let kind = body.get("kind").and_then(Value::as_str).unwrap_or_default();
        let item = kind == "ITEM";
        let refs = match body.get("refs") {
            Some(Value::Array(refs)) => refs.clone(),
            _ => Vec::new(),
        };
        let of_kind: Vec<&Goods> = GOODS.iter().filter(|g| g.kind.is_none() == item).collect();
        let hit = |g: &Goods| object(vec![("bt", Value::str(g.bt)), ("name", Value::str(g.name))]);
        let results = refs
            .iter()
            .enumerate()
            .map(|(index, reference)| {
                let by_bt = reference.get("bt").and_then(Value::as_str);
                let name = lower(reference.get("name").and_then(Value::as_str).unwrap_or_default().trim());
                let exact: Vec<&&Goods> =
                    of_kind.iter().filter(|g| by_bt.map_or(lower(g.name) == name, |bt| g.bt == bt)).collect();
                let near: Vec<&&Goods> =
                    of_kind.iter().filter(|g| !name.is_empty() && lower(g.name).contains(&name)).collect();
                let mut pairs = vec![("index", Value::int(index as i64))];
                if let Some(g) = exact.first() {
                    pairs.extend([("status", Value::str("resolved")), ("ref", hit(g))]);
                } else if !near.is_empty() && by_bt.is_none() {
                    pairs.extend([
                        ("status", Value::str("ambiguous")),
                        ("candidates", Value::Array(near.iter().map(|g| hit(g)).collect())),
                    ]);
                } else {
                    pairs.push(("status", Value::str("unmatched")));
                }
                object(pairs)
            })
            .collect();
        answer(200, object(vec![("results", Value::Array(results))]))
    }

    fn locations() -> Response {
        let items = PLACES.iter().map(|p| object(vec![("name", Value::str(*p))])).collect();
        answer(200, object(vec![("items", Value::Array(items))]))
    }

    /// The lot for these goods at this place in this unit, made when missing.
    fn lot_for(&mut self, unit: &str, bt: &str, place: &str, quality: i64, stolen: bool) -> usize {
        let wanted = lower(place);
        if let Some(i) = self.lots.iter().position(|l| {
            l.unit == unit && l.bt == bt && lower(&l.place) == wanted && l.quality == quality && l.stolen == stolen
        }) {
            return i;
        }
        let key = self.fresh_id("L");
        let place = PLACES.iter().find(|p| lower(p) == wanted).map_or(place.to_string(), |p| (*p).to_string());
        self.lots.push(lot(&key, unit, bt, &place, quality, stolen, Vec::new()));
        self.lots.len() - 1
    }

    /// Puts an amount into lot `at` for `holder`, merged with the holder's
    /// entry there when asked.
    fn put(&mut self, at: usize, holder: &str, amount: f64, merge: bool, note: &str) {
        let id = self.fresh_id("e");
        let target = &mut self.lots[at];
        match target.entries.iter_mut().find(|e| merge && e.holder == holder && !e.withheld) {
            Some(existing) => {
                existing.amount += amount;
                existing.version += 1;
            }
            None => {
                let mut fresh = entry(&id, holder, amount);
                fresh.note = note.to_string();
                target.entries.push(fresh);
            }
        }
        let key = target.key.clone();
        self.touch(&key);
    }

    fn find_entry(&self, unit: &str, id: &str) -> Option<(usize, usize)> {
        self.lots
            .iter()
            .enumerate()
            .filter(|(_, l)| l.unit == unit)
            .find_map(|(i, l)| l.entries.iter().position(|e| e.id == id).map(|k| (i, k)))
    }

    /// Takes `amount` out of an entry: from its free part, and from the
    /// reservations named in the operation.
    fn take(&mut self, at: (usize, usize), amount: f64, op: &Map) -> Result<(), &'static str> {
        let e = &mut self.lots[at.0].entries[at.1];
        if amount > e.amount + 1e-9 {
            return Err("INSUFFICIENT_STOCK");
        }
        let named = |key: &str, field: &str| -> Vec<(String, f64)> {
            match get(op, key) {
                Some(Value::Array(lines)) => lines
                    .iter()
                    .map(|l| {
                        (
                            l.get(field).and_then(Value::as_str).unwrap_or_default().to_string(),
                            number_of(l.get("quantity").and_then(|q| q.get("amount"))),
                        )
                    })
                    .collect(),
                _ => Vec::new(),
            }
        };
        let from_jobs = named("fromJobOrders", "jobOrder");
        let from_missions = named("fromMissions", "mission");
        let reserved_jobs: f64 = e.jobs.iter().map(|j| j.2).sum::<f64>() - from_jobs.iter().map(|j| j.1).sum::<f64>();
        let reserved_missions: f64 =
            e.missions.iter().map(|m| m.2).sum::<f64>() - from_missions.iter().map(|m| m.1).sum::<f64>();
        if amount > e.amount - reserved_jobs.max(0.0) + 1e-9 || amount > e.amount - reserved_missions.max(0.0) + 1e-9 {
            return Err("STOCK_EARMARKED");
        }
        for (id, take) in from_jobs {
            if let Some(job) = e.jobs.iter_mut().find(|j| j.0 == id) {
                job.2 = (job.2 - take).max(0.0);
            }
        }
        for (id, take) in from_missions {
            if let Some(mission) = e.missions.iter_mut().find(|m| m.0 == id) {
                mission.2 = (mission.2 - take).max(0.0);
            }
        }
        e.jobs.retain(|j| j.2 > 1e-9);
        e.missions.retain(|m| m.2 > 1e-9);
        e.amount -= amount;
        e.version += 1;
        Ok(())
    }

    /// Drops empty entries and lots after a change of lot `at`.
    fn tidy(&mut self, at: usize) {
        let key = self.lots[at].key.clone();
        self.lots[at].entries.retain(|e| e.amount > 1e-9);
        self.touch(&key);
        self.lots.retain(|l| !l.entries.is_empty());
    }

    fn apply(&mut self, op: &Map) -> Result<(), &'static str> {
        let name = get_str(op, "op").unwrap_or_default();
        let unit = get_str(op, "unit").unwrap_or_default().to_string();
        if !UNITS.iter().any(|u| u.0 == unit && u.4) {
            return Err("NOT_PERMITTED");
        }
        let merge = get(op, "mergeScu").is_some_and(truthy);
        if name == "book-in" {
            let holder = get_str(op, "holder").unwrap_or_default();
            if !World::roster(&unit).is_some_and(|r| r.iter().any(|(h, _)| *h == holder)) {
                return Err("HOLDER_UNKNOWN");
            }
            let bt = get(op, "material").and_then(|m| m.get("bt")).and_then(Value::as_str).unwrap_or_default();
            let Some(found) = goods(bt) else { return Err("UNMATCHED") };
            let piece = get(op, "quantity").and_then(|q| q.get("unit")).and_then(Value::as_str) == Some(PIECE);
            if piece != found.piece {
                return Err("UNIT_MISMATCH");
            }
            let place = get(op, "location").and_then(|l| l.get("name")).and_then(Value::as_str).unwrap_or_default();
            if !PLACES.iter().any(|p| lower(p) == lower(place.trim())) {
                return Err("LOCATION_UNKNOWN");
            }
            let quality = number_of(get(op, "quality")) as i64;
            let stolen = get(op, "stolen").is_some_and(truthy);
            let at = self.lot_for(&unit, bt, place.trim(), quality, stolen);
            let note = get_str(op, "note").unwrap_or_default().to_string();
            self.put(at, holder, quantity_of(op).unwrap_or(0.0), merge, &note);
            return Ok(());
        }
        let id = get_str(op, "entry").unwrap_or_default();
        let Some(at) = self.find_entry(&unit, id) else { return Err("NOT_PERMITTED") };
        let version = number_of(get(op, "version")) as i64;
        if self.lots[at.0].entries[at.1].version != version {
            return Err("VERSION_CONFLICT");
        }
        let (bt, place, quality, stolen) = {
            let l = &self.lots[at.0];
            (l.bt.clone(), l.place.clone(), l.quality, l.stolen)
        };
        let holder = self.lots[at.0].entries[at.1].holder.clone();
        match name {
            "book-out" | "sell" => {
                if name == "sell" && get_str(op, "terminal").is_none_or(|t| t.trim().is_empty()) {
                    return Err("UNMATCHED");
                }
                self.take(at, quantity_of(op).unwrap_or(0.0), op)?;
                self.tidy(at.0);
            }
            "move-location" => {
                let to = get(op, "location").and_then(|l| l.get("name")).and_then(Value::as_str).unwrap_or_default();
                if !PLACES.iter().any(|p| lower(p) == lower(to.trim())) {
                    return Err("LOCATION_UNKNOWN");
                }
                let amount = quantity_of(op).unwrap_or(0.0);
                self.take(at, amount, op)?;
                let target = self.lot_for(&unit, &bt, to.trim(), quality, stolen);
                self.put(target, &holder, amount, merge, "");
                let from = self.lots.iter().position(|l| {
                    l.place == place && l.bt == bt && l.unit == unit && l.quality == quality && l.stolen == stolen
                });
                if let Some(from) = from {
                    self.tidy(from);
                }
            }
            "move-holder" => {
                let to_unit = get_str(op, "toUnit").unwrap_or(&unit).to_string();
                let new_holder = get_str(op, "holder").unwrap_or_default().to_string();
                if !World::roster(&to_unit).is_some_and(|r| r.iter().any(|(h, _)| *h == new_holder)) {
                    return Err("HOLDER_UNKNOWN");
                }
                let amount = quantity_of(op).unwrap_or(0.0);
                self.take(at, amount, op)?;
                let key = self.lots[at.0].key.clone();
                let target = self.lot_for(&to_unit, &bt, &place, quality, stolen);
                self.put(target, &new_holder, amount, merge, "");
                if let Some(from) = self.lots.iter().position(|l| l.key == key) {
                    self.tidy(from);
                }
            }
            "move-pool" => {
                let to_unit = get_str(op, "toUnit").unwrap_or_default().to_string();
                if !UNITS.iter().any(|u| u.0 == to_unit && u.4) {
                    return Err("NOT_PERMITTED");
                }
                if !World::roster(&to_unit).is_some_and(|r| r.iter().any(|(h, _)| *h == holder)) {
                    return Err("HOLDER_UNKNOWN");
                }
                let whole_entry = self.lots[at.0].entries[at.1].amount;
                let amount = quantity_of(op).unwrap_or(whole_entry);
                let key = self.lots[at.0].key.clone();
                let target = self.lot_for(&to_unit, &bt, &place, quality, stolen);
                if quantity_of(op).is_none() {
                    let moved = self.lots[at.0].entries.remove(at.1);
                    self.lots[target].entries.push(moved);
                    let target_key = self.lots[target].key.clone();
                    self.touch(&target_key);
                } else {
                    self.take(at, amount, op)?;
                    self.put(target, &holder, amount, merge, "");
                }
                if let Some(from) = self.lots.iter().position(|l| l.key == key) {
                    self.tidy(from);
                } else {
                    self.touch(&key);
                }
            }
            "note" => {
                let e = &mut self.lots[at.0].entries[at.1];
                e.note = get_str(op, "note").unwrap_or_default().to_string();
                e.version += 1;
                let key = self.lots[at.0].key.clone();
                self.touch(&key);
            }
            "stolen" => {
                let to = get(op, "stolen").is_some_and(truthy);
                let whole_entry = self.lots[at.0].entries[at.1].amount;
                let amount = quantity_of(op).unwrap_or(whole_entry);
                let key = self.lots[at.0].key.clone();
                self.take(at, amount, op)?;
                let target = self.lot_for(&unit, &bt, &place, quality, to);
                self.put(target, &holder, amount, true, "");
                if let Some(from) = self.lots.iter().position(|l| l.key == key) {
                    self.tidy(from);
                }
            }
            "allocation-add" | "allocation-change" | "allocation-remove" => {
                let job = get_str(op, "jobOrder").map(str::to_string);
                let mission = get_str(op, "mission").map(str::to_string);
                let amount = quantity_of(op).unwrap_or(0.0);
                let e = &mut self.lots[at.0].entries[at.1];
                match (job, mission) {
                    (Some(id), None) => {
                        let Some(order) = ORDERS.iter().find(|o| o.0 == id && o.3 == bt) else {
                            return Err("EARMARK_TARGET_INVALID");
                        };
                        e.jobs.retain(|j| j.0 != id);
                        if name != "allocation-remove" {
                            e.jobs.push((id, order.1, amount, false));
                        }
                    }
                    (None, Some(id)) => {
                        let Some(found) = MISSIONS.iter().find(|m| m.0 == id) else {
                            return Err("EARMARK_TARGET_INVALID");
                        };
                        e.missions.retain(|m| m.0 != id);
                        if name != "allocation-remove" {
                            e.missions.push((id, found.1.to_string(), amount));
                        }
                    }
                    _ => return Err("EARMARK_TARGET_INVALID"),
                }
                let jobs: f64 = e.jobs.iter().map(|j| j.2).sum();
                let missions: f64 = e.missions.iter().map(|m| m.2).sum();
                if jobs > e.amount + 1e-9 || missions > e.amount + 1e-9 {
                    return Err("OVER_ALLOCATION");
                }
                e.version += 1;
                let key = self.lots[at.0].key.clone();
                self.touch(&key);
            }
            _ => return Err("UNMATCHED"),
        }
        Ok(())
    }

    fn changes(&mut self, body: &Value) -> Response {
        self.bodies.push(body.clone());
        let ops: Vec<Map> = match body.get("ops") {
            Some(Value::Array(ops)) => ops.iter().filter_map(Value::as_object).cloned().collect(),
            _ => Vec::new(),
        };
        if ops.iter().any(|op| quantity_of(op).is_some_and(|q| q > MASS_LIMIT)) {
            return answer(
                409,
                object(vec![
                    ("code", Value::str("MASS_CHANGE_CONFIRMATION_REQUIRED")),
                    ("confirmationUrl", Value::str("https://beispiel.profit-base.invalid/bestaetigen/demo")),
                ]),
            );
        }
        let (mut applied, mut results) = (0, Vec::new());
        for (index, op) in ops.iter().enumerate() {
            let before = self.lots.clone();
            match self.apply(op) {
                Ok(()) => applied += 1,
                Err(reason) => {
                    self.lots = before;
                    results.push(object(vec![
                        ("index", Value::int(index as i64)),
                        ("result", Value::str("rejected")),
                        ("reason", Value::str(reason)),
                    ]));
                }
            }
        }
        let refused = results.len() as i64;
        answer(
            200,
            object(vec![
                ("dryRun", Value::Bool(false)),
                ("applied", Value::int(applied)),
                ("unchanged", Value::int(0)),
                ("notApplied", Value::int(refused)),
                ("results", Value::Array(results)),
            ]),
        )
    }
}

impl Transport for DemoBasetool {
    fn send(&self, request: &Request) -> Result<Response, ApiError> {
        let mut world = lock(&self.world);
        let (path, query) = request.url.split_once('?').unwrap_or((request.url.as_str(), ""));
        let query = query_of(query);
        let arg = |name: &str| query.iter().find(|(k, _)| k == name).map(|(_, v)| v.clone());
        let body = request
            .body
            .as_ref()
            .and_then(|b| std::str::from_utf8(b).ok())
            .and_then(|t| jsonio::loads(t).ok())
            .unwrap_or(Value::Null);
        let shown = path.strip_prefix(API).or_else(|| path.strip_prefix(ISSUER)).unwrap_or(path);
        let line = if query.is_empty() {
            format!("{} {shown}", request.method)
        } else {
            let pairs: Vec<String> = query.iter().map(|(k, v)| format!("{k}={v}")).collect();
            format!("{} {shown}?{}", request.method, pairs.join("&"))
        };
        world.seen.push(line);
        if path == format!("{ISSUER}/.well-known/openid-configuration") {
            return Ok(answer(
                200,
                object(vec![
                    ("issuer", Value::str(ISSUER)),
                    ("device_authorization_endpoint", Value::str(format!("{ISSUER}/device"))),
                    ("token_endpoint", Value::str(format!("{ISSUER}/token"))),
                ]),
            ));
        }
        if path == format!("{ISSUER}/device") {
            let form =
                request.body.as_ref().and_then(|b| std::str::from_utf8(b).ok()).map(query_of).unwrap_or_default();
            let scope = form.iter().find(|(k, _)| k == "scope").map(|(_, v)| v.clone()).unwrap_or_default();
            world.asked = scope.split_whitespace().map(str::to_string).collect();
            world.polls = 0;
            return Ok(answer(
                200,
                object(vec![
                    ("device_code", Value::str(DEVICE_CODE)),
                    ("user_code", Value::str(USER_CODE)),
                    ("verification_uri", Value::str(format!("{ISSUER}/device"))),
                    // The Basetool sends a link with the code built in, too; it must never be shown.
                    ("verification_uri_complete", Value::str(format!("{ISSUER}/device?user_code={USER_CODE}"))),
                    ("interval", Value::int(5)),
                    ("expires_in", Value::int(600)),
                ]),
            ));
        }
        if path == format!("{ISSUER}/token") {
            let form =
                request.body.as_ref().and_then(|b| std::str::from_utf8(b).ok()).map(query_of).unwrap_or_default();
            let kind = form.iter().find(|(k, _)| k == "grant_type").map(|(_, v)| v.as_str()).unwrap_or_default();
            if kind.ends_with("device_code") {
                world.polls += 1;
                let failure = match world.login {
                    DemoLogin::Deny => Some("access_denied"),
                    DemoLogin::Expire => Some("expired_token"),
                    DemoLogin::Approve if world.polls < world.approve_at => Some("authorization_pending"),
                    DemoLogin::Approve => None,
                };
                if let Some(code) = failure {
                    return Ok(answer(400, object(vec![("error", Value::str(code))])));
                }
                world.granted = Some(
                    world
                        .asked
                        .iter()
                        .filter(|s| s.starts_with("exchange.") && s.as_str() != "exchange.connect")
                        .cloned()
                        .collect(),
                );
            }
            let rights = match &world.granted {
                Some(granted) => granted.clone(),
                None => DEFAULT_RIGHTS.iter().map(|s| (*s).to_string()).collect(),
            };
            let scope = format!("exchange.connect {} offline_access", rights.join(" "));
            return Ok(answer(
                200,
                object(vec![
                    ("token_type", Value::str("DPoP")),
                    ("access_token", Value::str("demo-access")),
                    ("refresh_token", Value::str("demo-refresh")),
                    ("expires_in", Value::int(300)),
                    ("scope", Value::str(scope)),
                ]),
            ));
        }
        let route = path.strip_prefix(API).unwrap_or("?");
        if let (true, Some((status, code))) = (route.is_empty(), world.service_error.clone()) {
            return Ok(problem(status, &code));
        }
        Ok(match (request.method.as_str(), route) {
            ("GET", "") => {
                let rights = match &world.granted {
                    Some(granted) => granted.clone(),
                    None => DEFAULT_RIGHTS.iter().map(|s| (*s).to_string()).collect(),
                };
                answer(
                    200,
                    object(vec![
                        ("installationId", Value::str("beispiel")),
                        ("capabilities", Value::Array(rights.into_iter().map(Value::Str).collect())),
                    ]),
                )
            }
            ("POST", "/me/installation") => answer(200, object(Vec::new())),
            ("POST", "/me/account-check") => answer(200, object(vec![("result", Value::str(world.account.clone()))])),
            ("GET", "/org-stock") => world.page(arg("cursor").as_deref()),
            ("GET", "/me/org-demand") => world.demand(),
            ("GET", "/org-stock/members") => world.members(&arg("unit").unwrap_or_default()),
            ("GET", "/org-stock/targets") => {
                world.targets(&arg("unit").unwrap_or_default(), &arg("material").unwrap_or_default())
            }
            ("GET", "/catalog/locations") => World::locations(),
            ("POST", "/catalog/resolve") => World::resolve(&body),
            ("POST", "/org-stock/changes") => world.changes(&body),
            _ => problem(404, "NOT_FOUND"),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::basetool::connection::system_clock;
    use crate::basetool::orgstock::{self, OrgStock, Resolved};

    #[test]
    fn reading_and_booking_without_the_network() {
        let demo = Arc::new(DemoBasetool::new());
        let conn = demo.connection(system_clock());
        let store = OrgStock::new();
        let counts = store.refresh(&conn, true, true, 1000.0).expect("refresh");
        assert_eq!(get(&counts, "logistician"), Some(&Value::int(2)));
        let (_, data) = store.current(1000.0);
        let data = data.expect("data");
        assert_eq!(data.units.len(), 3);
        assert!(data.can_write("u-alpha") && !data.can_write("u-industrie"));
        assert_eq!(data.rosters["u-alpha"].len(), 4);
        let (lot, entry) = data.entry("e-4").expect("entry");
        let op = orgstock::op_book_out(&lot.unit_id, entry, 2.0, &lot.unit_q, &Default::default(), &Default::default());
        let outcome = orgstock::send(&conn, vec![op]).expect("send");
        assert_eq!((outcome.applied, outcome.failed.len()), (1, 0));
        store.refresh(&conn, true, true, 1001.0).expect("feed");
        let (_, data) = store.current(1001.0);
        let data = data.expect("data");
        assert_eq!(data.entry("e-4").map(|(_, e)| e.amount), Some(30.0));
        // The feed brought another member's change: version 2 of e-1.
        assert_eq!(data.entry("e-1").map(|(_, e)| e.version), Some(2));
        let old_e1 = orgstock::Entry { version: 1, ..data.entry("e-1").expect("e-1").1.clone() };
        let conflict = orgstock::send(&conn, vec![orgstock::op_note("u-alpha", &old_e1, "x")]).expect("send");
        assert_eq!(conflict.reasons(), ["VERSION_CONFLICT"]);
        assert!(matches!(orgstock::resolve_material(&conn, "lara", None, false), Ok(Resolved::Several(_))));
        let members = demo.seen().iter().filter(|s| s.starts_with("GET /org-stock/members?unit=")).count();
        assert_eq!(members, 4, "two units with a roster, two fetches");
        assert!(demo.seen().iter().any(|s| s == "GET /org-stock?limit=500&cursor=c1"), "{:?}", demo.seen());
    }

    /// The area in a real pass of the sync: switched on, it fetches with
    /// the rights the Basetool granted; booking needs its own switch.
    #[test]
    fn the_area_runs_in_a_sync_pass() {
        use crate::basetool::orgstock::OrgStockArea;
        use crate::basetool::scopes::{SETTING_ORGSTOCK, SETTING_ORGSTOCK_WRITE};
        use crate::basetool::sync::{Setup, SyncHandle};
        let dir = std::env::temp_dir().join(format!("versekit-orgstock-pass-{}", crate::testdir::run_tag()));
        let _ = std::fs::remove_dir_all(&dir);
        let data = dir.to_string_lossy();
        let host = crate::paths::Host::new(crate::pypath::Flavor::native(), &[(crate::legacy::ENV_HOME, &data)], &data);
        let paths = Arc::new(crate::paths::Paths::new(host));
        assert!(paths.set_setting(SETTING_ORGSTOCK, Value::Bool(true)));
        let demo = Arc::new(DemoBasetool::new());
        let store = OrgStock::new();
        let handle = SyncHandle::without_worker(Setup {
            conn: Arc::new(demo.connection(system_clock())),
            paths: paths.clone(),
            clock: system_clock(),
            version: "5.0.0".into(),
            own_account: Arc::new(|| Some("PlayerOne".to_string())),
            waker: Arc::new(|| {}),
            areas: vec![Box::new(OrgStockArea { store: store.clone() })],
        });
        handle.pass_now().expect("pass");
        let (_, data) = store.current(system_clock()());
        let data = data.expect("fetched");
        assert!(data.details && !data.write, "booking is off: no member lists");
        assert!(data.rosters.is_empty());
        assert!(!demo.seen().iter().any(|s| s.contains("/org-stock/members")));
        assert!(paths.set_setting(SETTING_ORGSTOCK_WRITE, Value::Bool(true)));
        handle.pass_now().expect("pass");
        let data = store.current(system_clock()()).1.expect("fetched");
        assert!(data.write && data.can_write("u-alpha"));
        // Switched off: the next pass throws everything away.
        assert!(paths.set_setting(SETTING_ORGSTOCK, Value::Bool(false)));
        assert!(paths.set_setting("basetool_bedarf", Value::Bool(true)));
        let _ = handle.pass_now();
        assert!(store.current(system_clock()()).1.is_none(), "kept after switching off");
        let _ = std::fs::remove_dir_all(dir);
    }

    /// Disconnecting while a fetch runs: the fetch keeps nothing.
    #[test]
    fn forgetting_during_a_fetch_keeps_nothing() {
        struct Forgets {
            inner: Arc<DemoBasetool>,
            store: Arc<OrgStock>,
        }
        impl Transport for Forgets {
            fn send(&self, request: &Request) -> Result<Response, ApiError> {
                if request.url.contains("/org-stock?") {
                    self.store.forget();
                }
                self.inner.send(request)
            }
        }
        let demo = Arc::new(DemoBasetool::new());
        let store = OrgStock::new();
        let plain = demo.connection(system_clock());
        let forgets = Arc::new(Forgets { inner: demo.clone(), store: store.clone() });
        let config = plain.config().clone();
        let secrets = Arc::new(MemorySecrets::default());
        let _ = secrets.save(REFRESH_SECRET, "r");
        let conn =
            Connection::new(config, forgets, secrets.clone(), Arc::new(StoredKey { secrets }), "5.0.0", system_clock());
        store.refresh(&conn, true, false, 1000.0).expect("refresh");
        assert!(store.current(1000.0).1.is_none(), "kept a fetch that ran across forget");
        store.refresh(&plain, true, false, 1001.0).expect("refresh");
        assert!(store.current(1001.0).1.is_some(), "a normal fetch keeps its data");
    }

    fn scratch(name: &str) -> String {
        let dir = std::env::temp_dir().join(format!("versekit-demosync-{name}-{}", crate::testdir::run_tag()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("Ordner");
        dir.to_string_lossy().into_owned()
    }

    /// Waits for `done`, at most `seconds`.
    fn wait_for(seconds: u64, mut done: impl FnMut() -> bool) -> bool {
        let end = std::time::Instant::now() + std::time::Duration::from_secs(seconds);
        while std::time::Instant::now() < end {
            if done() {
                return true;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        done()
    }

    /// The whole way of the connection page: switch on, connect with the
    /// code, the rights follow the switches, the page of the area appears,
    /// switching off throws the data away, disconnecting leaves nothing.
    #[test]
    fn a_device_login_connects_and_the_switches_decide_what_is_there() {
        use crate::basetool::scopes::{SETTING_DEMAND, SETTING_ORGSTOCK};
        use crate::basetool::sync::{Login, Status};
        let folder = scratch("login");
        let world = DemoSync::new(&folder, Arc::new(|| {}), Some("PlayerOne".into()), false);
        world.demo.script_login(DemoLogin::Approve, 1);
        let sync = &world.sync;
        assert!(!sync.connection().connected());
        assert!(!sync.area_visible(SETTING_ORGSTOCK), "nothing is switched on");
        assert!(sync.set_setting(SETTING_ORGSTOCK, Value::Bool(true)));
        sync.connect();
        assert!(wait_for(10, || matches!(sync.login_state().get().1.as_deref(), Some(Login::Code { .. }))));
        let code = sync.login_state().get().1.expect("login");
        assert!(matches!(&*code, Login::Code { user_code, .. } if user_code == USER_CODE), "{code:?}");
        // Only the bare address, never the link with the code built in.
        assert!(
            matches!(&*code, Login::Code { verification_uri, .. } if *verification_uri == format!("{ISSUER}/device")),
            "{code:?}"
        );
        assert!(wait_for(20, || matches!(sync.login_state().get().1.as_deref(), Some(Login::Connected))));
        assert!(sync.connection().connected());
        assert_eq!(
            world.demo.granted(),
            Some(vec!["exchange.orgstock.read".into(), "exchange.orgstock.details".into()])
        );
        sync.pass_now().expect("pass");
        assert!(sync.area_visible(SETTING_ORGSTOCK), "switch, right and connection");
        assert!(world.org_stock.current(system_clock()()).1.is_some());
        assert!(sync.state().is_some(), "the state of the installation is there after a pass");
        // A second area without its right: its page stays away.
        assert!(sync.set_setting(SETTING_DEMAND, Value::Bool(true)));
        assert!(!sync.area_visible(SETTING_DEMAND));
        sync.pass_now().expect("pass");
        let status = sync.status().get().1.expect("status");
        assert_eq!(status.missing, [SETTING_DEMAND]);
        assert_eq!(status.code, "SCOPE_MISSING", "a new login would grant it");
        // Switched off: the data goes at once, without waiting for a pass.
        assert!(sync.set_setting(SETTING_ORGSTOCK, Value::Bool(false)));
        sync.switched();
        assert!(wait_for(5, || world.org_stock.current(system_clock()()).1.is_none()));
        assert!(!sync.area_visible(SETTING_ORGSTOCK));
        // Disconnecting: token, state and published data are gone.
        let id = status.installation_id.clone().expect("id");
        let file = crate::basetool::state::state_file(&world.paths, &id);
        assert!(std::path::Path::new(&file).exists());
        let job = sync.disconnect();
        assert!(job.wait(std::time::Duration::from_secs(10)).is_some());
        assert!(!sync.connection().connected());
        assert!(!std::path::Path::new(&file).exists());
        assert_eq!(sync.status().get().1.as_deref(), Some(&Status::default()));
        let _ = std::fs::remove_dir_all(folder);
    }

    /// A refused or expired code ends the login with its own state; nothing
    /// is connected.
    #[test]
    fn a_refused_or_expired_code_connects_nothing() {
        use crate::basetool::scopes::SETTING_ORGSTOCK;
        use crate::basetool::sync::Login;
        for (outcome, expected) in [(DemoLogin::Deny, Login::Denied), (DemoLogin::Expire, Login::Expired)] {
            let folder = scratch("refused");
            let world = DemoSync::new(&folder, Arc::new(|| {}), None, false);
            world.demo.script_login(outcome, 1);
            assert!(world.sync.set_setting(SETTING_ORGSTOCK, Value::Bool(true)));
            world.sync.connect();
            assert!(wait_for(20, || world.sync.login_state().get().1.as_deref() == Some(&expected)), "{outcome:?}");
            assert!(!world.sync.connection().connected());
            let _ = std::fs::remove_dir_all(folder);
        }
    }

    /// A connection the Basetool ended hides the page of an area although
    /// the rights of the last pass are still known: the stored token decides.
    #[test]
    fn a_revoked_connection_hides_the_pages_of_its_areas() {
        use crate::basetool::scopes::SETTING_ORGSTOCK;
        let folder = scratch("widerrufen");
        let world = DemoSync::new(&folder, Arc::new(|| {}), Some("PlayerOne".into()), false);
        let sync = &world.sync;
        assert!(sync.set_setting(SETTING_ORGSTOCK, Value::Bool(true)));
        sync.connection().secrets().save(REFRESH_SECRET, "demo-refresh").expect("token");
        sync.pass_now().expect("pass");
        assert!(sync.area_visible(SETTING_ORGSTOCK));
        world.demo.script_service_error(Some((401, "CLIENT_REVOKED")));
        assert!(sync.pass_now().is_err());
        assert!(sync.granted().iter().any(|s| s.ends_with("orgstock.read")), "the rights are still known");
        assert!(!sync.area_visible(SETTING_ORGSTOCK), "but the token is gone");
        let _ = std::fs::remove_dir_all(folder);
    }

    /// A switch is written under the write lock of the sync, not under the one
    /// of the thread that flips it (a preview locks its own thread), and the
    /// lock of that thread is back when the write is done.
    #[test]
    fn a_switch_is_written_under_the_lock_of_the_sync() {
        use crate::basetool::scopes::SETTING_ORGSTOCK;
        /// Opens the thread again, also when an assertion fails.
        struct Unlock;
        impl Drop for Unlock {
            fn drop(&mut self) {
                crate::writeguard::deny(false);
            }
        }
        let folder = scratch("sperre");
        let world = DemoSync::new(&folder, Arc::new(|| {}), None, false);
        let unlock = Unlock;
        crate::writeguard::deny(true);
        assert!(world.sync.set_setting(SETTING_ORGSTOCK, Value::Bool(true)), "the lock of the thread stopped it");
        assert!(world.sync.paths().setting_bool(SETTING_ORGSTOCK, false));
        assert!(
            !world.sync.paths().set_setting(SETTING_ORGSTOCK, Value::Bool(false)),
            "the lock of the thread is back"
        );
        drop(unlock);
        let _ = std::fs::remove_dir_all(folder);
    }

    /// The account check stops a pass with its reason; confirming goes on.
    #[test]
    fn an_unknown_account_stops_until_it_is_confirmed() {
        use crate::basetool::scopes::SETTING_ORGSTOCK;
        use crate::basetool::sync::Phase;
        let folder = scratch("account");
        let world = DemoSync::new(&folder, Arc::new(|| {}), Some("PlayerOne".into()), false);
        let sync = &world.sync;
        assert!(sync.set_setting(SETTING_ORGSTOCK, Value::Bool(true)));
        // Already connected without a login: the made-up default rights.
        sync.connection().secrets().save(REFRESH_SECRET, "demo-refresh").expect("token");
        world.demo.script_account("unknown");
        sync.pass_now().expect("pass");
        let status = sync.status().get().1.expect("status");
        assert_eq!((status.phase, status.code.as_str()), (Phase::Stopped, "account_unknown"));
        assert!(world.org_stock.current(system_clock()()).1.is_none(), "stopped before any area");
        assert!(sync.confirm_account());
        sync.pass_now().expect("pass");
        assert_eq!(sync.status().get().1.expect("status").phase, Phase::Ok);
        assert!(world.org_stock.current(system_clock()()).1.is_some());
        let _ = std::fs::remove_dir_all(folder);
    }

    #[test]
    fn a_large_booking_is_held_for_the_browser() {
        let demo = Arc::new(DemoBasetool::new());
        let conn = demo.connection(system_clock());
        let op = orgstock::op_book_in(
            "u-alpha",
            "r-nova",
            object(vec![("bt", Value::str("bt-laranite"))]),
            object(vec![("name", Value::str("Area18"))]),
            500,
            false,
            9000.0,
            SCU,
            "",
            false,
        );
        let held = orgstock::send(&conn, vec![op]).expect("send");
        assert!(held.held);
        assert_eq!(held.confirm_url.as_deref(), Some("https://beispiel.profit-base.invalid/bestaetigen/demo"));
    }
}
