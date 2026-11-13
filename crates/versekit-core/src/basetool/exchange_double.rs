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

//! A stand-in of the Basetool for the three areas blueprints, stock and ships,
//! in memory: the feeds with cursors and deletion marks, `catalog/resolve`,
//! `catalog/locations`, the change sets with their refusals (`VERSION_CONFLICT`,
//! `REMOVED_ELSEWHERE`), a held mass change and scripted faults (maintenance
//! with `Retry-After`, an expired cursor). Nothing leaves the process: it
//! answers as a [`Transport`] with a login of its own, and a key and a refresh
//! token only in memory.
//!
//! It is what the tests of the areas stand on, and the same logic serves a
//! server on `127.0.0.1` for the test that goes through the real transport
//! ([`ExchangeDouble::handle`]). It is not the Basetool: it knows no DPoP
//! check (the flows of the connection are tested against a server that does),
//! and its rules are the ones the sync guide names, nothing more.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use crate::basetool::config::Config;
use crate::basetool::connection::{Clock, Connection};
use crate::basetool::error::ApiError;
use crate::basetool::http::{Request, Response, Transport};
use crate::basetool::key::StoredKey;
use crate::basetool::secrets::{MemorySecrets, REFRESH_SECRET, SecretStore};
use crate::jsonio::{self, Map, Value};

/// The address of the made-up issuer and API (the `invalid` domain exists
/// nowhere).
pub const ISSUER: &str = "https://beispiel.profit-base.invalid/auth/realms/iri";
pub const API: &str = "https://beispiel.profit-base.invalid/exchange/v1";
/// The installation that asks.
pub const INSTALLATION: &str = "inst-doppel";
/// Another installation of the same member (removes things "elsewhere").
pub const OTHER_INSTALLATION: &str = "inst-anders";
/// A confirmation page of the made-up Basetool.
pub const CONFIRM_URL: &str = "https://beispiel.profit-base.invalid/confirm/abc";

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

fn object(pairs: Vec<(&str, Value)>) -> Value {
    Value::Object(pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
}

fn answer(status: u16, body: Value) -> Response {
    Response { status, headers: Vec::new(), body: jsonio::dumps(&body, None, false).into_bytes() }
}

fn problem(status: u16, code: &str) -> Response {
    answer(status, object(vec![("code", Value::str(code))]))
}

fn decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => match u8::from_str_radix(&text[i + 1..i + 3], 16) {
                Ok(byte) => {
                    out.push(byte);
                    i += 3;
                    continue;
                }
                Err(_) => out.push(b'%'),
            },
            b'+' => out.push(b' '),
            other => out.push(other),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn query_of(query: &str) -> Vec<(String, String)> {
    query.split('&').filter_map(|pair| pair.split_once('=')).map(|(k, v)| (decode(k), decode(v))).collect()
}

fn amount_of(value: Option<&Value>) -> f64 {
    match value {
        Some(Value::Int(n)) => n.parse().unwrap_or(0.0),
        Some(Value::Float(x)) => *x,
        _ => 0.0,
    }
}

fn text_of(value: Option<&Value>) -> String {
    value.and_then(Value::as_str).unwrap_or_default().to_string()
}

fn lower(text: &str) -> String {
    text.trim().to_lowercase()
}

/// A blueprint over there.
#[derive(Clone, Debug)]
pub struct BpEntry {
    pub bt: String,
    pub name: String,
    pub default: bool,
    pub acquired: Option<String>,
}

/// A lot over there.
#[derive(Clone, Debug)]
pub struct Lot {
    pub key: String,
    pub bt: String,
    pub name: String,
    pub place: String,
    pub quality: i64,
    pub stolen: bool,
    pub amount: f64,
    pub unit: String,
    pub item: bool,
    /// What lies in the pools of units: unit and amount (the feed says so in `split`).
    pub pools: Vec<(String, f64)>,
}

/// A ship over there.
#[derive(Clone, Debug)]
pub struct Ship {
    pub id: String,
    pub external: Option<String>,
    pub type_bt: String,
    pub type_name: String,
    pub insurance: Value,
    pub fitted: bool,
    pub name: String,
    pub version: i64,
    pub unit: Option<String>,
}

/// A refinery order over there.
#[derive(Clone, Debug)]
pub struct RefineryOrder {
    pub id: String,
    pub client_key: Option<String>,
    pub status: String,
    pub place: String,
    pub ends_at: String,
    /// Output material, yield in units, quality.
    pub goods: Vec<(String, i64, i64)>,
    pub game_data: i64,
    pub picked_up: bool,
}

/// The refineries the made-up Basetool offers for refinery orders (also places of `catalog/locations`).
pub const REFINERIES: [&str; 2] = ["CRU-L1 Ambitious Dream Station", "ARC-L1 Wide Forest Station"];

#[derive(Clone, Debug)]
struct Fault {
    route: String,
    status: u16,
    code: String,
    retry_after: Option<u64>,
    times: u32,
}

/// What happened to a key: present again (`gone_by` none) or gone, removed
/// by an installation.
#[derive(Clone, Debug)]
struct Log {
    revision: u64,
    key: String,
    gone_by: Option<String>,
}

struct World {
    rights: Vec<String>,
    revision: u64,
    bps: Vec<BpEntry>,
    bp_log: Vec<Log>,
    /// Blueprints removed by another installation.
    removed_elsewhere: Vec<String>,
    catalog_bps: Vec<(String, String)>,
    lots: Vec<Lot>,
    lot_log: Vec<Log>,
    lot_elsewhere: Vec<String>,
    materials: Vec<(String, String, bool)>,
    places: Vec<String>,
    ships: Vec<Ship>,
    ship_log: Vec<Log>,
    ship_elsewhere: Vec<String>,
    ship_types: Vec<(String, String)>,
    units: Vec<(String, String)>,
    next: u64,
    /// Method and path with query of every request.
    seen: Vec<String>,
    /// Route and body of every change set that came in.
    bodies: Vec<(String, Value)>,
    faults: Vec<Fault>,
    hold: Vec<String>,
    expire_cursor: bool,
    account: String,
    /// No `installationId` in the service document.
    no_installation: bool,
    /// The next token requests fail: status, the `error` of the answer (empty for none), how often.
    token_faults: Vec<(u16, String, u32)>,
    /// What a change set of a route does to things the Basetool holds besides it (offers on the exchange,
    /// missions), as the fields of its answer: route, field, count.
    effects: Vec<(String, &'static str, i64)>,
    /// A lot another member changes just before the next stock change set is read.
    race: Vec<(String, String, i64, bool, f64)>,
    /// Work done once when the next request to a route arrives (the player writes a file meanwhile).
    touch: Vec<(String, Touch)>,
    /// The next change of the unit of a ship is refused: id of the ship and the reason of the answer.
    unit_refusals: Vec<(String, String)>,
    /// Quantity changes of a material or item (lower case name) that are refused, with the reason.
    material_refusals: Vec<(String, String)>,
    /// The class of an item in the catalogue (`scRecord`, lower case) and its `bt`.
    records: Vec<(String, String)>,
    /// The answers to change sets by their `Idempotency-Key`, with the body that came under it.
    replays: Vec<(String, Option<Vec<u8>>, Response)>,
    /// The next change set on each of these routes is applied, but its answer is lost on the way back.
    lose_answers: Vec<String>,
    orders: Vec<RefineryOrder>,
    order_log: Vec<Log>,
    bank: BankWorld,
}

/// The Kartellbank over there.
struct BankWorld {
    /// The bank switch of the Basetool for connected applications.
    on: bool,
    /// Accounts as the route answers them.
    accounts: Vec<Value>,
    /// The member's requests by id, and the log of their changes for the feed.
    requests: Vec<(String, Value)>,
    log: Vec<Log>,
    /// The queue entries; `None`: the member is no bank staff.
    queue: Option<Vec<Value>>,
}

type Touch = Box<dyn FnOnce() + Send>;

/// The stand-in.
pub struct ExchangeDouble {
    world: Mutex<World>,
}

impl Default for ExchangeDouble {
    fn default() -> Self {
        ExchangeDouble::new()
    }
}

impl ExchangeDouble {
    pub fn new() -> ExchangeDouble {
        let rights = [
            "exchange.blueprints.read",
            "exchange.blueprints.write",
            "exchange.stock.read",
            "exchange.stock.write",
            "exchange.hangar.read",
            "exchange.hangar.write",
            "exchange.orgunits.read",
        ];
        let blueprints = ["Aether Rifle", "Bastion Helm", "Corsair Legs", "Dust Gloves", "Ember Core", "Forge Pack"];
        let materials = [
            ("bt-iron", "Iron", false),
            ("bt-gold", "Gold", false),
            ("bt-copper", "Copper", false),
            ("bt-hadanite", "Hadanite", false),
            ("bt-pistol", "Pistol", true),
            ("bt-medpen", "Medpen", true),
            ("bt-gold-ore", "Gold (Ore)", false),
            ("bt-iron-ore", "Iron (Ore)", false),
            ("bt-quantainium-raw", "Quantainium (Raw)", false),
        ];
        ExchangeDouble {
            world: Mutex::new(World {
                rights: rights.iter().map(|r| (*r).to_string()).collect(),
                revision: 1,
                bps: Vec::new(),
                bp_log: Vec::new(),
                removed_elsewhere: Vec::new(),
                catalog_bps: blueprints
                    .iter()
                    .map(|n| (format!("bt-{}", n.to_lowercase().replace(' ', "-")), (*n).to_string()))
                    .collect(),
                lots: Vec::new(),
                lot_log: Vec::new(),
                lot_elsewhere: Vec::new(),
                materials: materials.iter().map(|(b, n, i)| ((*b).into(), (*n).into(), *i)).collect(),
                places: ["Orbituary", "Lorville", "Area18"].iter().map(|p| (*p).to_string()).collect(),
                ships: Vec::new(),
                ship_log: Vec::new(),
                ship_elsewhere: Vec::new(),
                ship_types: [("bt-ship-avenger", "Avenger Titan"), ("bt-ship-cutlass", "Cutlass Black")]
                    .iter()
                    .map(|(b, n)| ((*b).into(), (*n).into()))
                    .collect(),
                units: Vec::new(),
                next: 100,
                seen: Vec::new(),
                bodies: Vec::new(),
                faults: Vec::new(),
                hold: Vec::new(),
                expire_cursor: false,
                account: "match".into(),
                no_installation: false,
                token_faults: Vec::new(),
                effects: Vec::new(),
                race: Vec::new(),
                touch: Vec::new(),
                unit_refusals: Vec::new(),
                material_refusals: Vec::new(),
                records: Vec::new(),
                replays: Vec::new(),
                lose_answers: Vec::new(),
                orders: Vec::new(),
                order_log: Vec::new(),
                bank: BankWorld { on: true, accounts: Vec::new(), requests: Vec::new(), log: Vec::new(), queue: None },
            }),
        }
    }

    /// The bank switch of the Basetool for connected applications.
    pub fn set_bank(&self, on: bool) {
        self.world().bank.on = on;
    }

    /// An account the member views; gives its id.
    pub fn add_account(&self, no: &str, name: &str, balance: i64, requestable: bool) -> String {
        let mut world = self.world();
        let id = format!("acc-{}", world.next);
        world.next += 1;
        world.bank.accounts.push(object(vec![
            ("accountId", Value::str(&id)),
            ("accountNo", Value::str(no)),
            ("name", Value::str(name)),
            ("type", Value::str("ORG_UNIT")),
            ("balance", Value::int(balance)),
            ("balanceTarget", Value::Null),
            ("delta30d", Value::int(0)),
            ("sparkline", Value::Array(vec![Value::int(balance)])),
            ("requestable", Value::Bool(requestable)),
            ("ownLimit", Value::Null),
            ("approvalExempt", Value::Bool(false)),
        ]));
        id
    }

    /// The queue of a bank employee with entries of these ids; `None` makes the member no bank staff.
    pub fn set_queue(&self, ids: Option<&[&str]>) {
        self.world().bank.queue = ids.map(|ids| {
            ids.iter()
                .map(|id| {
                    object(vec![
                        ("id", Value::str(*id)),
                        ("type", Value::str("WITHDRAWAL")),
                        ("amount", Value::int(2500)),
                        ("requesterHandle", Value::Null),
                        ("mayConfirm", Value::Bool(true)),
                        ("link", Value::str(format!("https://beispiel.profit-base.invalid/bank/requests/{id}"))),
                    ])
                })
                .collect()
        });
    }

    /// The bank decides a request (or the member does so in the web): its status changes.
    pub fn decide_bank_request(&self, id: &str, status: &str) {
        let mut world = self.world();
        world.bank_change(id, |request| set_field(request, "status", Value::str(status)));
    }

    /// The member's requests as the Basetool holds them.
    pub fn bank_requests(&self) -> Vec<Value> {
        self.world().bank.requests.iter().map(|(_, r)| r.clone()).collect()
    }

    /// A refinery order over there, made in the web (`key` none) or by an installation; gives its id.
    pub fn add_refinery_order(
        &self,
        key: Option<&str>,
        place: &str,
        status: &str,
        ends_at: &str,
        goods: &[(&str, i64, i64)],
    ) -> String {
        let mut world = self.world();
        let id = format!("ro-{}", world.next);
        world.next += 1;
        world.orders.push(RefineryOrder {
            id: id.clone(),
            client_key: key.map(str::to_string),
            status: status.into(),
            place: place.into(),
            ends_at: ends_at.into(),
            goods: goods.iter().map(|(m, a, q)| ((*m).to_string(), *a, *q)).collect(),
            game_data: 1,
            picked_up: false,
        });
        let revision = world.bump();
        world.order_log.push(Log { revision, key: id.clone(), gone_by: None });
        id
    }

    /// Changes an order as the web does (status, end); its counter moves.
    pub fn change_refinery_order(&self, id: &str, status: Option<&str>, ends_at: Option<&str>) {
        let mut world = self.world();
        if let Some(order) = world.orders.iter_mut().find(|o| o.id == id) {
            if let Some(status) = status {
                order.status = status.into();
            }
            if let Some(ends_at) = ends_at {
                order.ends_at = ends_at.into();
            }
            order.game_data += 1;
        }
        let revision = world.bump();
        world.order_log.push(Log { revision, key: id.into(), gone_by: None });
    }

    /// An order deleted in the web: its deletion mark comes with the feed.
    pub fn delete_refinery_order(&self, id: &str) {
        let mut world = self.world();
        world.orders.retain(|o| o.id != id);
        let revision = world.bump();
        world.order_log.push(Log { revision, key: id.into(), gone_by: None });
    }

    /// The refinery orders over there.
    pub fn refinery_orders(&self) -> Vec<RefineryOrder> {
        self.world().orders.clone()
    }

    /// The next change set on `route` is applied, but VerseKit gets a 503 instead of the answer.
    pub fn lose_next_answer(&self, route: &str) {
        self.world().lose_answers.push(route.into());
    }

    /// An item of the catalogue under `name` (which may be another than the game's) and its class: `catalog/resolve`
    /// of kind `ITEM` finds it by `scRecord` before the name.
    pub fn add_item(&self, bt: &str, name: &str, record: &str) {
        let mut world = self.world();
        world.materials.push((bt.into(), name.into(), true));
        world.records.push((lower(record), bt.into()));
    }

    /// The connection to it: a key and a refresh token in memory, nothing in
    /// the system's store.
    pub fn connection(self: &Arc<Self>, clock: Clock) -> Connection {
        let config = Config {
            issuer: ISSUER.into(),
            api: API.into(),
            client_id: "versekit".into(),
            ca: String::new(),
            sandbox: true,
            overridden: true,
        };
        let secrets = Arc::new(MemorySecrets::default());
        let _ = secrets.save(REFRESH_SECRET, "doppel-refresh");
        let keys = Arc::new(StoredKey { secrets: secrets.clone() });
        Connection::new(config, self.clone(), secrets, keys, "5.0.0", clock)
    }

    fn world(&self) -> MutexGuard<'_, World> {
        lock(&self.world)
    }

    // ---------------------------------------------------------- for the tests

    /// The rights the service document grants.
    pub fn set_rights(&self, rights: &[&str]) {
        self.world().rights = rights.iter().map(|r| (*r).to_string()).collect();
    }

    /// What the account check answers.
    pub fn script_account(&self, answer: &str) {
        self.world().account = answer.to_string();
    }

    /// The service document has no `installationId`.
    pub fn script_no_installation(&self, on: bool) {
        self.world().no_installation = on;
    }

    /// The next `times` requests whose route starts with `route` fail with
    /// this status and code (`retry_after` in seconds).
    pub fn fail(&self, route: &str, status: u16, code: &str, retry_after: Option<u64>, times: u32) {
        self.world().faults.push(Fault { route: route.into(), status, code: code.into(), retry_after, times });
    }

    /// Takes the faults away.
    pub fn heal(&self) {
        let mut world = self.world();
        world.faults.clear();
        world.token_faults.clear();
        world.unit_refusals.clear();
        world.material_refusals.clear();
    }

    /// Every change of the quantity of a material or item named `name` is refused with `reason` (`AMBIGUOUS`)
    /// until [`heal`](Self::heal).
    pub fn refuse_material(&self, name: &str, reason: &str) {
        self.world().material_refusals.push((lower(name), reason.into()));
    }

    /// The next change that sets the unit of the ship `id` is refused with `reason`
    /// (`ORG_UNIT_NOT_ALLOWED`, `ORG_UNIT_VERSION_CONFLICT`); the ship stays as it is.
    pub fn refuse_unit(&self, id: &str, reason: &str) {
        self.world().unit_refusals.push((id.into(), reason.into()));
    }

    /// The next `times` token requests fail with `status`; `error` is the
    /// `error` of the answer (`invalid_grant`), empty for an answer without one.
    pub fn fail_token(&self, status: u16, error: &str, times: u32) {
        self.world().token_faults.push((status, error.into(), times));
    }

    /// Another member changes this lot just before the next stock change set
    /// is read (so `expectedQuantity` no longer matches: `VERSION_CONFLICT`).
    pub fn race_lot(&self, material: &str, place: &str, quality: i64, stolen: bool, amount: f64) {
        self.world().race.push((material.into(), place.into(), quality, stolen, amount));
    }

    /// `work` runs once when the next request to `route` arrives, before it is answered.
    pub fn on_next_request(&self, route: &str, work: impl FnOnce() + Send + 'static) {
        self.world().touch.push((route.into(), Box::new(work)));
    }

    /// The next change set on `route` is held until the member confirms it in
    /// the browser.
    pub fn hold_next(&self, route: &str) {
        self.world().hold.push(route.into());
    }

    /// The next feed request with a cursor finds the cursor expired.
    pub fn expire_cursor(&self) {
        self.world().expire_cursor = true;
    }

    /// Method and path with query of every request so far.
    pub fn seen(&self) -> Vec<String> {
        self.world().seen.clone()
    }

    /// Every change set so far with its route.
    pub fn bodies(&self) -> Vec<(String, Value)> {
        self.world().bodies.clone()
    }

    /// The change sets of one route, the `ops` of each.
    pub fn ops_sent(&self, route: &str) -> Vec<Vec<Value>> {
        self.world().bodies.iter().filter(|(r, _)| r == route).map(|(_, body)| ops_of(body)).collect()
    }

    /// A blueprint that stands there. `bt` is made from the name.
    pub fn add_blueprint(&self, name: &str, default: bool, acquired: Option<&str>) -> String {
        let mut world = self.world();
        let bt = format!("bt-{}", lower(name).replace(' ', "-"));
        if !world.catalog_bps.iter().any(|(b, _)| *b == bt) {
            world.catalog_bps.push((bt.clone(), name.to_string()));
        }
        if !world.bps.iter().any(|b| b.bt == bt) {
            world.bps.push(BpEntry {
                bt: bt.clone(),
                name: name.into(),
                default,
                acquired: acquired.map(str::to_string),
            });
            let revision = world.bump();
            world.bp_log.push(Log { revision, key: bt.clone(), gone_by: None });
            world.removed_elsewhere.retain(|b| *b != bt);
        }
        bt
    }

    /// The member removes a blueprint in the browser (another installation).
    pub fn remove_blueprint_elsewhere(&self, name: &str) {
        let mut world = self.world();
        let bt = format!("bt-{}", lower(name).replace(' ', "-"));
        if let Some(at) = world.bps.iter().position(|b| b.bt == bt) {
            world.bps.remove(at);
            let revision = world.bump();
            world.bp_log.push(Log { revision, key: bt.clone(), gone_by: Some(OTHER_INSTALLATION.into()) });
            world.removed_elsewhere.push(bt);
        }
    }

    /// The names of the blueprints over there, sorted.
    pub fn blueprint_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.world().bps.iter().map(|b| b.name.clone()).collect();
        names.sort();
        names
    }

    /// A lot over there (made, or its amount set).
    pub fn set_lot(&self, material: &str, place: &str, quality: i64, stolen: bool, amount: f64) {
        let mut world = self.world();
        let (bt, name, item) = world
            .materials
            .iter()
            .find(|(_, n, _)| lower(n) == lower(material))
            .cloned()
            .unwrap_or((format!("bt-{}", lower(material)), material.into(), false));
        let unit = if item { "PIECE" } else { "SCU" };
        let existing = world
            .lots
            .iter()
            .position(|l| l.bt == bt && lower(&l.place) == lower(place) && l.quality == quality && l.stolen == stolen);
        let revision = world.bump();
        match existing {
            Some(at) => {
                world.lots[at].amount = amount;
                let key = world.lots[at].key.clone();
                world.lot_log.push(Log { revision, key, gone_by: None });
            }
            None => {
                let key = format!("L{}", world.next);
                world.next += 1;
                world.lots.push(Lot {
                    key: key.clone(),
                    bt,
                    name,
                    place: place.into(),
                    quality,
                    stolen,
                    amount,
                    unit: unit.into(),
                    item,
                    pools: Vec::new(),
                });
                world.lot_log.push(Log { revision, key, gone_by: None });
            }
        }
    }

    /// The member removes a lot in the browser (another installation).
    pub fn remove_lot_elsewhere(&self, material: &str, place: &str, quality: i64, stolen: bool) {
        let mut world = self.world();
        let Some(at) = world.lots.iter().position(|l| {
            lower(&l.name) == lower(material)
                && lower(&l.place) == lower(place)
                && l.quality == quality
                && l.stolen == stolen
        }) else {
            return;
        };
        let lot = world.lots.remove(at);
        let revision = world.bump();
        world.lot_elsewhere.push(format!("{}|{}|{}|{}", lot.bt, lower(&lot.place), lot.quality, i32::from(lot.stolen)));
        world.lot_log.push(Log { revision, key: lot.key, gone_by: Some(OTHER_INSTALLATION.into()) });
    }

    /// The amount over there of a lot, 0 if there is none.
    pub fn lot_amount(&self, material: &str, place: &str, quality: i64, stolen: bool) -> f64 {
        let world = self.world();
        world
            .lots
            .iter()
            .find(|l| {
                lower(&l.name) == lower(material)
                    && lower(&l.place) == lower(place)
                    && l.quality == quality
                    && l.stolen == stolen
            })
            .map_or(0.0, |l| l.amount)
    }

    /// The lots over there.
    pub fn lots(&self) -> Vec<Lot> {
        self.world().lots.clone()
    }

    /// A ship over there.
    pub fn add_ship(&self, type_name: &str, external: Option<&str>, insurance: Value, name: &str) -> String {
        let mut world = self.world();
        let type_bt = world
            .ship_types
            .iter()
            .find(|(_, n)| lower(n) == lower(type_name))
            .map(|(b, _)| b.clone())
            .unwrap_or_else(|| format!("bt-ship-{}", lower(type_name).replace(' ', "-")));
        if !world.ship_types.iter().any(|(b, _)| *b == type_bt) {
            world.ship_types.push((type_bt.clone(), type_name.into()));
        }
        let id = format!("S{}", world.next);
        world.next += 1;
        world.ships.push(Ship {
            id: id.clone(),
            external: external.map(str::to_string),
            type_bt,
            type_name: type_name.into(),
            insurance,
            fitted: false,
            name: name.into(),
            version: 1,
            unit: None,
        });
        let revision = world.bump();
        world.ship_log.push(Log { revision, key: id.clone(), gone_by: None });
        id
    }

    /// The member changes the insurance of a ship in the browser.
    pub fn set_insurance(&self, id: &str, insurance: Value) {
        let mut world = self.world();
        if let Some(at) = world.ships.iter().position(|s| s.id == id) {
            world.ships[at].insurance = insurance;
            world.ships[at].version += 1;
            let revision = world.bump();
            world.ship_log.push(Log { revision, key: id.into(), gone_by: None });
        }
    }

    /// The change sets of `route` that apply something also report `count` of `field` (`offersReduced`,
    /// `offersRemoved`, `detachedFromMissions`), as the Basetool does when it touches offers or missions.
    pub fn script_effect(&self, route: &str, field: &'static str, count: i64) {
        self.world().effects.push((route.to_string(), field, count));
    }

    /// The member renames a ship in the browser.
    pub fn rename_ship(&self, id: &str, name: &str) {
        self.change_ship(id, |ship| ship.name = name.into());
    }

    /// The member marks a ship ready for missions in the browser.
    pub fn set_fitted(&self, id: &str, fitted: bool) {
        self.change_ship(id, |ship| ship.fitted = fitted);
    }

    /// The member moves a ship to another unit in the browser.
    pub fn set_ship_unit(&self, id: &str, unit: Option<&str>) {
        self.change_ship(id, |ship| ship.unit = unit.map(str::to_string));
    }

    fn change_ship(&self, id: &str, change: impl FnOnce(&mut Ship)) {
        let mut world = self.world();
        if let Some(at) = world.ships.iter().position(|s| s.id == id) {
            change(&mut world.ships[at]);
            world.ships[at].version += 1;
            let revision = world.bump();
            world.ship_log.push(Log { revision, key: id.into(), gone_by: None });
        }
    }

    /// The member removes a ship in the browser.
    pub fn remove_ship_elsewhere(&self, id: &str) {
        let mut world = self.world();
        if let Some(at) = world.ships.iter().position(|s| s.id == id) {
            let external = world.ships[at].external.clone();
            world.ships.remove(at);
            let revision = world.bump();
            world.ship_log.push(Log { revision, key: id.into(), gone_by: Some(OTHER_INSTALLATION.into()) });
            if let Some(external) = external {
                world.ship_elsewhere.push(external);
            }
        }
    }

    /// The ships over there.
    pub fn ships(&self) -> Vec<Ship> {
        self.world().ships.clone()
    }

    /// The units of the member.
    pub fn set_units(&self, units: &[(&str, &str)]) {
        self.world().units = units.iter().map(|(i, n)| ((*i).to_string(), (*n).to_string())).collect();
    }

    // ------------------------------------------------------------- answering

    /// Answers one request to the API (`route` without the API address).
    pub fn handle(&self, method: &str, route: &str, query: &[(String, String)], body: &Value) -> Response {
        let mut world = self.world();
        let arg = |name: &str| query.iter().find(|(k, _)| k == name).map(|(_, v)| v.clone());
        if let Some(at) = world.touch.iter().position(|(r, _)| r == route) {
            (world.touch.remove(at).1)();
        }
        if let Some(at) = world.faults.iter().position(|f| route.starts_with(&f.route) && f.times > 0) {
            world.faults[at].times -= 1;
            let fault = world.faults[at].clone();
            // A scripted fault brings a detail and a correlation id, as the Basetool's problem documents do.
            let mut response = answer(
                fault.status,
                object(vec![
                    ("code", Value::str(&fault.code)),
                    ("detail", Value::str(format!("Detail-Geheimnis {route}"))),
                    ("correlationId", Value::str("korrelation-geheim")),
                ]),
            );
            if let Some(seconds) = fault.retry_after {
                response.headers.push(("Retry-After".into(), seconds.to_string()));
            }
            return response;
        }
        match (method, route) {
            ("GET", "") => {
                let mut pairs = vec![("capabilities", Value::Array(world.rights.iter().map(Value::str).collect()))];
                if !world.no_installation {
                    pairs.insert(0, ("installationId", Value::str(INSTALLATION)));
                }
                answer(200, object(pairs))
            }
            ("POST", "/me/installation") => answer(200, object(Vec::new())),
            ("POST", "/me/account-check") => answer(200, object(vec![("result", Value::str(&world.account))])),
            ("GET", "/me/org-units") => {
                let items = world
                    .units
                    .iter()
                    .map(|(id, name)| {
                        object(vec![
                            ("id", Value::str(id)),
                            ("name", Value::str(name)),
                            ("kind", Value::str("SQUADRON")),
                        ])
                    })
                    .collect();
                answer(200, object(vec![("items", Value::Array(items))]))
            }
            ("GET", "/catalog/locations") => {
                let items = world
                    .places
                    .iter()
                    .map(String::as_str)
                    .chain(REFINERIES)
                    .map(|p| object(vec![("name", Value::str(p))]))
                    .collect();
                answer(200, object(vec![("items", Value::Array(items))]))
            }
            ("GET", "/me/refinery-orders") => {
                // The snapshot holds the open orders only, the feed every change.
                let cursor = arg("cursor");
                let items: Vec<(String, Value)> = world
                    .orders
                    .iter()
                    .filter(|o| cursor.is_some() || o.status == "OPEN" || o.status == "IN_PROGRESS")
                    .map(|o| (o.id.clone(), o.json()))
                    .collect();
                let (log, revision, expire) =
                    (world.order_log.clone(), world.revision, std::mem::take(&mut world.expire_cursor));
                feed(&cursor, &items, &log, revision, expire)
            }
            ("POST", "/catalog/resolve") => world.resolve(body),
            ("GET", "/me/blueprints") => {
                let items: Vec<(String, Value)> = world
                    .bps
                    .iter()
                    .map(|b| {
                        let mut pairs = vec![
                            ("key", Value::str(&b.bt)),
                            ("ref", object(vec![("name", Value::str(&b.name)), ("bt", Value::str(&b.bt))])),
                            ("isDefault", Value::Bool(b.default)),
                        ];
                        if let Some(at) = &b.acquired {
                            pairs.push(("acquiredAt", Value::str(at)));
                        }
                        (b.bt.clone(), object(pairs))
                    })
                    .collect();
                let (log, revision, expire) =
                    (world.bp_log.clone(), world.revision, std::mem::take(&mut world.expire_cursor));
                feed(&arg("cursor"), &items, &log, revision, expire)
            }
            ("GET", "/me/stock") => {
                let items: Vec<(String, Value)> = world.lots.iter().map(|l| (l.key.clone(), l.json())).collect();
                let (log, revision, expire) =
                    (world.lot_log.clone(), world.revision, std::mem::take(&mut world.expire_cursor));
                feed(&arg("cursor"), &items, &log, revision, expire)
            }
            ("GET", "/me/ships") => {
                let items: Vec<(String, Value)> = world.ships.iter().map(|s| (s.id.clone(), s.json())).collect();
                let (log, revision, expire) =
                    (world.ship_log.clone(), world.revision, std::mem::take(&mut world.expire_cursor));
                feed(&arg("cursor"), &items, &log, revision, expire)
            }
            (_, bank) if bank.starts_with("/bank/") => {
                if !world.bank.on {
                    let mut response = problem(503, "BANK_EXCHANGE_DISABLED");
                    response.headers.push(("Retry-After".into(), "300".into()));
                    return response;
                }
                if method == "POST" {
                    world.bodies.push((bank.to_string(), body.clone()));
                }
                world.bank_answer(method, bank, &arg("cursor"), body)
            }
            ("POST", changes) if changes.ends_with("/changes") => {
                world.bodies.push((changes.to_string(), body.clone()));
                if let Some(at) = world.hold.iter().position(|r| r == changes) {
                    world.hold.remove(at);
                    return answer(
                        409,
                        object(vec![
                            ("code", Value::str("MASS_CHANGE_CONFIRMATION_REQUIRED")),
                            ("confirmationUrl", Value::str(CONFIRM_URL)),
                        ]),
                    );
                }
                match changes {
                    "/me/blueprints/changes" => world.change_blueprints(body),
                    "/me/stock/changes" => {
                        for (material, place, quality, stolen, amount) in std::mem::take(&mut world.race) {
                            world.race_now(&material, &place, quality, stolen, amount);
                        }
                        world.change_stock(body)
                    }
                    "/me/ships/changes" => world.change_ships(body),
                    "/me/refinery-orders/changes" => world.change_refinery(body),
                    _ => problem(404, "NOT_FOUND"),
                }
            }
            _ => problem(404, "NOT_FOUND"),
        }
    }
}

impl Lot {
    fn shared(&self) -> f64 {
        self.pools.iter().map(|(_, amount)| amount).sum()
    }

    fn json(&self) -> Value {
        let quantity = |amount: f64| object(vec![("amount", Value::Float(amount)), ("unit", Value::str(&self.unit))]);
        let pools: Vec<Value> = self
            .pools
            .iter()
            .map(|(unit, amount)| {
                object(vec![("orgUnit", object(vec![("id", Value::str(unit))])), ("quantity", quantity(*amount))])
            })
            .collect();
        object(vec![
            ("key", Value::str(&self.key)),
            ("kind", Value::str(if self.item { "ITEM" } else { "MATERIAL" })),
            ("material", object(vec![("bt", Value::str(&self.bt)), ("name", Value::str(&self.name))])),
            ("location", object(vec![("name", Value::str(&self.place))])),
            ("quality", Value::int(self.quality)),
            ("stolen", Value::Bool(self.stolen)),
            ("quantity", object(vec![("amount", Value::Float(self.amount)), ("unit", Value::str(&self.unit))])),
            (
                "split",
                object(vec![("personal", quantity(self.amount - self.shared())), ("pools", Value::Array(pools))]),
            ),
        ])
    }
}

impl RefineryOrder {
    fn json(&self) -> Value {
        let goods: Vec<Value> = self
            .goods
            .iter()
            .enumerate()
            .map(|(i, (name, amount, quality))| {
                object(vec![
                    ("id", Value::str(format!("{}-g{i}", self.id))),
                    ("output", object(vec![("name", Value::str(name))])),
                    ("inputQuantity", Value::int(0)),
                    ("outputQuantity", Value::int(*amount)),
                    ("quality", Value::int(*quality)),
                ])
            })
            .collect();
        let mut pairs = vec![
            ("id", Value::str(&self.id)),
            ("status", Value::str(&self.status)),
            ("versions", object(vec![("gameData", Value::int(self.game_data)), ("finances", Value::int(0))])),
            ("location", object(vec![("name", Value::str(&self.place))])),
            ("endsAt", Value::str(&self.ends_at)),
            ("goods", Value::Array(goods)),
        ];
        if let Some(key) = &self.client_key {
            pairs.push(("clientKey", Value::str(key)));
        }
        if self.picked_up {
            pairs.push(("pickedUpAt", Value::str(&self.ends_at)));
        }
        object(pairs)
    }
}

impl Ship {
    fn json(&self) -> Value {
        let mut pairs = vec![
            ("shipId", Value::str(&self.id)),
            ("version", Value::int(self.version)),
            ("shipType", object(vec![("bt", Value::str(&self.type_bt)), ("name", Value::str(&self.type_name))])),
            ("insurance", self.insurance.clone()),
            ("fitted", Value::Bool(self.fitted)),
            ("name", Value::str(&self.name)),
        ];
        if let Some(external) = &self.external {
            pairs.push(("externalId", Value::str(external)));
        }
        if let Some(unit) = &self.unit {
            pairs.push(("orgUnit", object(vec![("id", Value::str(unit))])));
        }
        object(pairs)
    }
}

/// A page of a feed: the snapshot without a cursor, else what changed since
/// the revision the cursor names (`c<revision>`).
fn feed(cursor: &Option<String>, items: &[(String, Value)], log: &[Log], revision: u64, expire: bool) -> Response {
    let next = format!("c{revision}");
    let Some(cursor) = cursor else {
        return answer(
            200,
            object(vec![
                ("items", Value::Array(items.iter().map(|(_, v)| v.clone()).collect())),
                ("hasMore", Value::Bool(false)),
                ("nextCursor", Value::str(&next)),
            ]),
        );
    };
    if expire {
        return problem(410, "CURSOR_EXPIRED");
    }
    let since: u64 = cursor.trim_start_matches('c').parse().unwrap_or(0);
    let mut keys: Vec<&String> = Vec::new();
    for entry in log.iter().filter(|l| l.revision > since) {
        if !keys.contains(&&entry.key) {
            keys.push(&entry.key);
        }
    }
    let mut changed = Vec::new();
    let mut removed = Vec::new();
    for key in keys {
        match items.iter().find(|(k, _)| k == key) {
            Some((_, item)) => changed.push(item.clone()),
            None => {
                let by = log.iter().rev().find(|l| &l.key == key).and_then(|l| l.gone_by.clone());
                let mut pairs = vec![("key", Value::str(key))];
                if let Some(by) = by {
                    pairs.push(("removedBy", object(vec![("installationId", Value::str(by))])));
                }
                removed.push(object(pairs));
            }
        }
    }
    answer(
        200,
        object(vec![
            ("items", Value::Array(changed)),
            ("removed", Value::Array(removed)),
            ("hasMore", Value::Bool(false)),
            ("nextCursor", Value::str(&next)),
        ]),
    )
}

fn results(applied: i64, unchanged: i64, results: Vec<Value>, cursor: u64) -> Response {
    results_with(applied, unchanged, results, cursor, &[])
}

/// [`results`] with the counts of what the change set did besides.
fn results_with(
    applied: i64,
    unchanged: i64,
    results: Vec<Value>,
    cursor: u64,
    effects: &[(&'static str, i64)],
) -> Response {
    let mut pairs = vec![
        ("applied", Value::int(applied)),
        ("unchanged", Value::int(unchanged)),
        ("results", Value::Array(results)),
        ("cursor", Value::str(format!("c{cursor}"))),
    ];
    pairs.extend(effects.iter().map(|(field, count)| (*field, Value::int(*count))));
    answer(200, object(pairs))
}

fn failure(index: usize, kind: &str, reason: &str) -> Value {
    let mut pairs = vec![("index", Value::int(index as i64)), ("result", Value::str(kind))];
    if !reason.is_empty() {
        pairs.push(("reason", Value::str(reason)));
    }
    object(pairs)
}

fn ops_of(body: &Value) -> Vec<Value> {
    match body.get("ops") {
        Some(Value::Array(ops)) => ops.clone(),
        _ => Vec::new(),
    }
}

fn set_field(value: &mut Value, key: &str, new: Value) {
    if let Value::Object(pairs) = value {
        match pairs.iter_mut().find(|(k, _)| k == key) {
            Some((_, old)) => *old = new,
            None => pairs.push((key.into(), new)),
        }
    }
}

impl World {
    /// Changes a request of the member and logs it for the feed; its version grows.
    fn bank_change(&mut self, id: &str, change: impl FnOnce(&mut Value)) {
        let Some(at) = self.bank.requests.iter().position(|(k, _)| k == id) else { return };
        let version = amount_of(self.bank.requests[at].1.get("version")) as i64;
        change(&mut self.bank.requests[at].1);
        set_field(&mut self.bank.requests[at].1, "version", Value::int(version + 1));
        let revision = self.bump();
        self.bank.log.push(Log { revision, key: id.into(), gone_by: None });
    }

    /// The bank routes, the switch already looked at.
    fn bank_answer(&mut self, method: &str, route: &str, cursor: &Option<String>, body: &Value) -> Response {
        let pending = self
            .bank
            .requests
            .iter()
            .filter(|(_, r)| r.get("status").and_then(Value::as_str) == Some("PENDING"))
            .count();
        match (method, route) {
            ("GET", "/bank/accounts") => answer(
                200,
                object(vec![
                    ("items", Value::Array(self.bank.accounts.clone())),
                    ("removed", Value::Array(Vec::new())),
                    ("hasMore", Value::Bool(false)),
                    ("nextCursor", Value::Null),
                ]),
            ),
            ("GET", "/bank/requests") => {
                let since: u64 = cursor.as_ref().map_or(0, |c| c.trim_start_matches('c').parse().unwrap_or(0));
                let items: Vec<Value> = match cursor {
                    None => self.bank.requests.iter().map(|(_, r)| r.clone()).collect(),
                    Some(_) => {
                        let mut keys: Vec<&String> = Vec::new();
                        for entry in self.bank.log.iter().filter(|l| l.revision > since) {
                            if !keys.contains(&&entry.key) {
                                keys.push(&entry.key);
                            }
                        }
                        keys.iter()
                            .filter_map(|k| self.bank.requests.iter().find(|(id, _)| id == *k).map(|(_, r)| r.clone()))
                            .collect()
                    }
                };
                answer(
                    200,
                    object(vec![
                        ("items", Value::Array(items)),
                        ("removed", Value::Array(Vec::new())),
                        ("hasMore", Value::Bool(false)),
                        ("nextCursor", Value::str(format!("c{}", self.revision))),
                        ("openRequestCap", Value::int(20)),
                    ]),
                )
            }
            ("GET", "/bank/queue") => match &self.bank.queue {
                None => problem(403, "NOT_BANK_STAFF"),
                Some(open) => answer(
                    200,
                    object(vec![
                        ("open", Value::Array(open.clone())),
                        ("toApprove", Value::Array(Vec::new())),
                        ("oldestOverdue", Value::Bool(false)),
                        ("hasMore", Value::Bool(false)),
                        ("nextCursor", Value::Null),
                    ]),
                ),
            },
            ("GET", path) if path.ends_with("/bookings") => {
                // The note names the account, so a test sees whose bookings came.
                let account = path.trim_start_matches("/bank/accounts/").trim_end_matches("/bookings");
                let booking = object(vec![
                    ("postingId", Value::str(format!("p-{account}"))),
                    ("type", Value::str("DEPOSIT")),
                    ("amount", Value::int(1000)),
                    ("createdAt", Value::str("2026-11-08T19:30:00Z")),
                    ("note", Value::str(decode(account))),
                    ("counterpartyHandle", Value::Null),
                ]);
                answer(200, object(vec![("items", Value::Array(vec![booking])), ("hasMore", Value::Bool(false))]))
            }
            ("GET", path) if path.ends_with("/balance-series") => {
                let point = object(vec![("date", Value::str("2026-11-08")), ("balance", Value::int(1000))]);
                answer(200, object(vec![("points", Value::Array(vec![point])), ("balanceTarget", Value::Null)]))
            }
            ("POST", "/bank/requests") => {
                if pending >= 20 {
                    return problem(409, "BANK_OPEN_REQUEST_CAP");
                }
                let account = text_of(body.get("sourceAccountId"));
                let Some(source) = self
                    .bank
                    .accounts
                    .iter()
                    .find(|a| a.get("accountId").and_then(Value::as_str) == Some(account.as_str()))
                    .cloned()
                else {
                    return problem(404, "NOT_FOUND");
                };
                let id = format!("req-{}", self.next);
                self.next += 1;
                let amount = amount_of(body.get("amount")) as i64;
                let request = object(vec![
                    ("id", Value::str(&id)),
                    ("version", Value::int(0)),
                    ("status", Value::str("PENDING")),
                    ("createdAt", Value::str(format!("2026-11-09T12:00:{:02}Z", self.next % 60))),
                    ("type", Value::str(text_of(body.get("type")))),
                    ("amount", Value::int(amount)),
                    ("note", Value::str(text_of(body.get("note")))),
                    (
                        "account",
                        object(vec![
                            ("id", Value::str(&account)),
                            ("no", source.get("accountNo").cloned().unwrap_or(Value::Null)),
                            ("name", source.get("name").cloned().unwrap_or(Value::Null)),
                        ]),
                    ),
                    ("client", Value::str("versekit")),
                ]);
                self.bank.requests.push((id.clone(), request));
                let revision = self.bump();
                self.bank.log.push(Log { revision, key: id.clone(), gone_by: None });
                answer(
                    201,
                    object(vec![
                        ("id", Value::str(&id)),
                        ("status", Value::str("PENDING")),
                        ("version", Value::int(0)),
                        ("amount", Value::int(amount)),
                    ]),
                )
            }
            ("POST", path) if path.ends_with("/correction") || path.ends_with("/cancel") => {
                let id = path.trim_start_matches("/bank/requests/").split('/').next().unwrap_or_default().to_string();
                let Some((_, request)) = self.bank.requests.iter().find(|(k, _)| *k == id).cloned() else {
                    return problem(404, "NOT_FOUND");
                };
                if request.get("status").and_then(Value::as_str) != Some("PENDING") {
                    return problem(409, "BANK_REQUEST_NOT_PENDING");
                }
                if amount_of(request.get("version")) != amount_of(body.get("version")) {
                    return problem(409, "VERSION_CONFLICT");
                }
                let cancel = path.ends_with("/cancel");
                let body = body.clone();
                self.bank_change(&id, |r| {
                    if cancel {
                        set_field(r, "status", Value::str("CANCELLED"));
                    } else {
                        for key in ["amount", "note", "justification"] {
                            if let Some(v) = body.get(key) {
                                set_field(r, key, v.clone());
                            }
                        }
                    }
                });
                let after = self.bank.requests.iter().find(|(k, _)| *k == id).map(|(_, r)| r.clone()).unwrap_or(Value::Null);
                answer(
                    200,
                    object(vec![
                        ("id", Value::str(&id)),
                        ("status", after.get("status").cloned().unwrap_or(Value::Null)),
                        ("version", after.get("version").cloned().unwrap_or(Value::Null)),
                        ("amount", after.get("amount").cloned().unwrap_or(Value::Null)),
                    ]),
                )
            }
            _ => problem(404, "NOT_FOUND"),
        }
    }

    fn bump(&mut self) -> u64 {
        self.revision += 1;
        self.revision
    }

    /// Sets the amount of an existing lot, or makes the lot.
    fn race_now(&mut self, material: &str, place: &str, quality: i64, stolen: bool, amount: f64) {
        let existing = self.lots.iter().position(|l| {
            lower(&l.name) == lower(material)
                && lower(&l.place) == lower(place)
                && l.quality == quality
                && l.stolen == stolen
        });
        let revision = self.bump();
        if let Some(at) = existing {
            self.lots[at].amount = amount;
            let key = self.lots[at].key.clone();
            self.lot_log.push(Log { revision, key, gone_by: None });
        }
    }

    fn resolve(&self, body: &Value) -> Response {
        let kind = text_of(body.get("kind"));
        let refs = match body.get("refs") {
            Some(Value::Array(refs)) => refs.clone(),
            _ => Vec::new(),
        };
        let catalog: Vec<(String, String)> = match kind.as_str() {
            "BLUEPRINT" => self.catalog_bps.clone(),
            "SHIP_TYPE" => self.ship_types.clone(),
            "MATERIAL" => {
                self.materials.iter().filter(|(_, _, item)| !item).map(|(b, n, _)| (b.clone(), n.clone())).collect()
            }
            "ITEM" => {
                self.materials.iter().filter(|(_, _, item)| *item).map(|(b, n, _)| (b.clone(), n.clone())).collect()
            }
            _ => Vec::new(),
        };
        let out = refs
            .iter()
            .enumerate()
            .map(|(index, reference)| {
                let record = lower(&text_of(reference.get("scRecord")));
                let by_record = self.records.iter().find(|(r, _)| kind == "ITEM" && *r == record).map(|(_, b)| b.as_str());
                let by_bt = reference.get("bt").and_then(Value::as_str).or(by_record);
                let name = lower(&text_of(reference.get("name")));
                let hits: Vec<&(String, String)> =
                    catalog.iter().filter(|(bt, n)| by_bt.map_or(lower(n) == name, |wanted| wanted == bt)).collect();
                let mut pairs = vec![("index", Value::int(index as i64))];
                match hits.as_slice() {
                    [(bt, n)] => pairs.extend([
                        ("status", Value::str("resolved")),
                        ("ref", object(vec![("bt", Value::str(bt)), ("name", Value::str(n))])),
                    ]),
                    [] => pairs.push(("status", Value::str("unmatched"))),
                    _ => pairs.push(("status", Value::str("ambiguous"))),
                }
                object(pairs)
            })
            .collect();
        answer(200, object(vec![("results", Value::Array(out))]))
    }

    fn change_blueprints(&mut self, body: &Value) -> Response {
        let (mut applied, mut unchanged) = (0, 0);
        let mut report = Vec::new();
        for (index, op) in ops_of(body).iter().enumerate() {
            match text_of(op.get("op")).as_str() {
                "add" => {
                    let bt = text_of(op.get("ref").and_then(|r| r.get("bt")));
                    if !self.catalog_bps.iter().any(|(b, _)| *b == bt) {
                        report.push(failure(index, "unmatched", "UNMATCHED"));
                    } else if self.bps.iter().any(|b| b.bt == bt) {
                        unchanged += 1;
                        report.push(failure(index, "unchanged", ""));
                    } else if self.removed_elsewhere.contains(&bt)
                        && !matches!(op.get("override"), Some(Value::Bool(true)))
                    {
                        report.push(failure(index, "rejected", "REMOVED_ELSEWHERE"));
                    } else {
                        let name =
                            self.catalog_bps.iter().find(|(b, _)| *b == bt).map(|(_, n)| n.clone()).unwrap_or_default();
                        let acquired = op.get("acquiredAt").and_then(Value::as_str).map(str::to_string);
                        self.bps.push(BpEntry { bt: bt.clone(), name, default: false, acquired });
                        let revision = self.bump();
                        self.bp_log.push(Log { revision, key: bt.clone(), gone_by: None });
                        self.removed_elsewhere.retain(|b| *b != bt);
                        applied += 1;
                    }
                }
                "remove" => {
                    let bt = text_of(op.get("key"));
                    match self.bps.iter().position(|b| b.bt == bt && !b.default) {
                        Some(at) => {
                            self.bps.remove(at);
                            let revision = self.bump();
                            self.bp_log.push(Log { revision, key: bt, gone_by: Some(INSTALLATION.into()) });
                            applied += 1;
                        }
                        None => report.push(failure(index, "rejected", "NOT_FOUND")),
                    }
                }
                _ => report.push(failure(index, "rejected", "UNKNOWN_OP")),
            }
        }
        results(applied, unchanged, report, self.revision)
    }

    /// `share` and `unshare`: the personal part against the pool of a unit.
    fn move_into_pool(&mut self, op: &Value, share: bool) -> Result<(), &'static str> {
        let bt = text_of(op.get("material").and_then(|m| m.get("bt")));
        let place = text_of(op.get("location").and_then(|l| l.get("name")));
        let quality = match op.get("quality") {
            Some(Value::Int(n)) => n.parse().unwrap_or(0),
            _ => 0,
        };
        let stolen = matches!(op.get("stolen"), Some(Value::Bool(true)));
        let unit = text_of(op.get("orgUnit").and_then(|u| u.get("id")));
        let quantity = amount_of(op.get("quantity").and_then(|q| q.get("amount")));
        let expected = amount_of(op.get("expectedQuantity").and_then(|q| q.get("amount")));
        let at = self
            .lots
            .iter()
            .position(|l| l.bt == bt && lower(&l.place) == lower(&place) && l.quality == quality && l.stolen == stolen)
            .ok_or("UNMATCHED")?;
        if !self.units.iter().any(|(id, _)| *id == unit) {
            return Err("NOT_A_MEMBER");
        }
        let held = self.lots[at].pools.iter().find(|(u, _)| *u == unit).map_or(0.0, |(_, a)| *a);
        let have = if share { self.lots[at].amount - self.lots[at].shared() } else { held };
        if (have - expected).abs() > 1e-9 {
            return Err("VERSION_CONFLICT");
        }
        if quantity > have + 1e-9 {
            return Err("QUANTITY_ABOVE_EXPECTED");
        }
        let rest = if share { held + quantity } else { held - quantity };
        let lot = &mut self.lots[at];
        lot.pools.retain(|(u, _)| *u != unit);
        if rest > 1e-9 {
            lot.pools.push((unit, rest));
        }
        let key = lot.key.clone();
        let revision = self.bump();
        self.lot_log.push(Log { revision, key, gone_by: None });
        Ok(())
    }

    fn change_stock(&mut self, body: &Value) -> Response {
        let (mut applied, mut unchanged) = (0, 0);
        let mut report = Vec::new();
        for (index, op) in ops_of(body).iter().enumerate() {
            let kind = text_of(op.get("op"));
            if kind == "share" || kind == "unshare" {
                match self.move_into_pool(op, kind == "share") {
                    Ok(()) => applied += 1,
                    Err(reason) => report.push(failure(index, "rejected", reason)),
                }
                continue;
            }
            if kind != "set-quantity" {
                report.push(failure(index, "rejected", "UNKNOWN_OP"));
                continue;
            }
            let material = op.get("material");
            let bt = text_of(material.and_then(|m| m.get("bt")));
            let named = lower(&text_of(material.and_then(|m| m.get("name"))));
            if let Some((_, reason)) = self.material_refusals.iter().find(|(name, _)| *name == named) {
                report.push(failure(index, "rejected", reason));
                continue;
            }
            let place = text_of(op.get("location").and_then(|l| l.get("name")));
            if !self.places.iter().any(|p| lower(p) == lower(&place)) {
                report.push(failure(index, "rejected", "LOCATION_UNKNOWN"));
                continue;
            }
            let quality = match op.get("quality") {
                Some(Value::Int(n)) => n.parse().unwrap_or(0),
                _ => 0,
            };
            let stolen = matches!(op.get("stolen"), Some(Value::Bool(true)));
            let quantity = amount_of(op.get("quantity").and_then(|q| q.get("amount")));
            let expected = amount_of(op.get("expectedQuantity").and_then(|q| q.get("amount")));
            let unit = text_of(op.get("quantity").and_then(|q| q.get("unit")));
            let position = self.lots.iter().position(|l| {
                l.bt == bt && lower(&l.place) == lower(&place) && l.quality == quality && l.stolen == stolen
            });
            let current = position.map_or(0.0, |at| self.lots[at].amount);
            let identity = format!("{bt}|{}|{quality}|{}", lower(&place), i32::from(stolen));
            if (current - expected).abs() > 1e-9 {
                let elsewhere = position.is_none() && self.lot_elsewhere.contains(&identity);
                let overridden = matches!(op.get("override"), Some(Value::Bool(true)));
                if elsewhere && overridden {
                    // The member says: mine counts. Goes on.
                } else if elsewhere {
                    report.push(failure(index, "rejected", "REMOVED_ELSEWHERE"));
                    continue;
                } else {
                    report.push(failure(index, "rejected", "VERSION_CONFLICT"));
                    continue;
                }
            }
            if (current - quantity).abs() < 1e-9 {
                unchanged += 1;
                report.push(failure(index, "unchanged", ""));
                continue;
            }
            let revision = self.bump();
            match (position, quantity > 0.0) {
                (Some(at), true) => {
                    self.lots[at].amount = quantity;
                    let key = self.lots[at].key.clone();
                    self.lot_log.push(Log { revision, key, gone_by: None });
                }
                (Some(at), false) => {
                    let lot = self.lots.remove(at);
                    self.lot_log.push(Log { revision, key: lot.key, gone_by: Some(INSTALLATION.into()) });
                }
                (None, true) => {
                    let name = self
                        .materials
                        .iter()
                        .find(|(b, _, _)| *b == bt)
                        .map(|(_, n, _)| n.clone())
                        .unwrap_or_else(|| text_of(material.and_then(|m| m.get("name"))));
                    let item = self.materials.iter().find(|(b, _, _)| *b == bt).is_some_and(|(_, _, i)| *i);
                    let key = format!("L{}", self.next);
                    self.next += 1;
                    self.lots.push(Lot {
                        key: key.clone(),
                        bt: bt.clone(),
                        name,
                        place: place.clone(),
                        quality,
                        stolen,
                        amount: quantity,
                        unit,
                        item,
                        pools: Vec::new(),
                    });
                    self.lot_elsewhere.retain(|e| *e != identity);
                    self.lot_log.push(Log { revision, key, gone_by: None });
                }
                (None, false) => {}
            }
            applied += 1;
        }
        let effects = self.effects_of("/me/stock/changes", applied);
        results_with(applied, unchanged, report, self.revision, &effects)
    }

    /// The effects scripted for a route, while something was applied.
    fn effects_of(&self, route: &str, applied: i64) -> Vec<(&'static str, i64)> {
        if applied == 0 {
            return Vec::new();
        }
        self.effects.iter().filter(|(r, _, _)| r == route).map(|(_, field, count)| (*field, *count)).collect()
    }

    fn change_ships(&mut self, body: &Value) -> Response {
        let mut applied = 0;
        let mut report = Vec::new();
        for (index, op) in ops_of(body).iter().enumerate() {
            match text_of(op.get("op")).as_str() {
                "link" => {
                    let id = text_of(op.get("shipId"));
                    let external = text_of(op.get("externalId"));
                    match self.ships.iter().position(|s| s.id == id) {
                        Some(at) => {
                            self.ships[at].external = Some(external);
                            self.ships[at].version += 1;
                            let revision = self.bump();
                            self.ship_log.push(Log { revision, key: id, gone_by: None });
                            applied += 1;
                        }
                        None => report.push(failure(index, "rejected", "NOT_FOUND")),
                    }
                }
                "upsert" => {
                    let id = text_of(op.get("shipId"));
                    if id.is_empty() {
                        let external = text_of(op.get("externalId"));
                        if self.ship_elsewhere.contains(&external)
                            && !matches!(op.get("override"), Some(Value::Bool(true)))
                        {
                            report.push(failure(index, "rejected", "REMOVED_ELSEWHERE"));
                            continue;
                        }
                        let type_bt = text_of(op.get("shipType").and_then(|t| t.get("bt")));
                        let Some((_, type_name)) = self.ship_types.iter().find(|(b, _)| *b == type_bt).cloned() else {
                            report.push(failure(index, "rejected", "UNMATCHED"));
                            continue;
                        };
                        let new_id = format!("S{}", self.next);
                        self.next += 1;
                        self.ships.push(Ship {
                            id: new_id.clone(),
                            external: Some(external.clone()),
                            type_bt,
                            type_name,
                            insurance: op.get("insurance").cloned().unwrap_or(Value::Null),
                            fitted: matches!(op.get("fitted"), Some(Value::Bool(true))),
                            name: text_of(op.get("name")),
                            version: 1,
                            unit: op.get("orgUnit").and_then(Value::as_str).map(str::to_string),
                        });
                        self.ship_elsewhere.retain(|e| *e != external);
                        let revision = self.bump();
                        self.ship_log.push(Log { revision, key: new_id, gone_by: None });
                        applied += 1;
                    } else {
                        let version = match op.get("version") {
                            Some(Value::Int(n)) => n.parse().unwrap_or(0),
                            _ => 0,
                        };
                        let refused =
                            op.get("orgUnit").and_then(|_| self.unit_refusals.iter().position(|(s, _)| *s == id));
                        if let Some(at) = refused {
                            let (_, reason) = self.unit_refusals.remove(at);
                            report.push(failure(index, "rejected", &reason));
                            continue;
                        }
                        match self.ships.iter().position(|s| s.id == id) {
                            Some(at) if self.ships[at].version == version => {
                                let ship = &mut self.ships[at];
                                if let Some(insurance) = op.get("insurance") {
                                    ship.insurance = insurance.clone();
                                }
                                if let Some(name) = op.get("name").and_then(Value::as_str) {
                                    ship.name = name.into();
                                }
                                if let Some(Value::Bool(fitted)) = op.get("fitted") {
                                    ship.fitted = *fitted;
                                }
                                if let Some(unit) = op.get("orgUnit") {
                                    ship.unit = unit.as_str().map(str::to_string);
                                }
                                ship.version += 1;
                                let revision = self.bump();
                                self.ship_log.push(Log { revision, key: id, gone_by: None });
                                applied += 1;
                            }
                            Some(_) => report.push(failure(index, "rejected", "VERSION_CONFLICT")),
                            None => report.push(failure(index, "rejected", "NOT_FOUND")),
                        }
                    }
                }
                "remove" => {
                    let id = text_of(op.get("shipId"));
                    match self.ships.iter().position(|s| s.id == id) {
                        Some(at) => {
                            self.ships.remove(at);
                            let revision = self.bump();
                            self.ship_log.push(Log { revision, key: id, gone_by: Some(INSTALLATION.into()) });
                            applied += 1;
                        }
                        None => report.push(failure(index, "rejected", "NOT_FOUND")),
                    }
                }
                _ => report.push(failure(index, "rejected", "UNKNOWN_OP")),
            }
        }
        let effects = self.effects_of("/me/ships/changes", applied);
        results_with(applied, 0, report, self.revision, &effects)
    }

    /// The refinery change set: `create`, `update` of the timer, `pickup` without store lines, by the rules of the
    /// resource (keys, counters, refineries, raw inputs, units). Store lines are refused: Verse-Kit never sends them.
    fn change_refinery(&mut self, body: &Value) -> Response {
        let (mut applied, mut unchanged) = (0, 0);
        let mut report = Vec::new();
        for (index, op) in ops_of(body).iter().enumerate() {
            let key = text_of(op.get("clientKey"));
            let id = text_of(op.get("id"));
            let found = self.orders.iter().position(|o| {
                (!id.is_empty() && o.id == id) || (id.is_empty() && !key.is_empty() && o.client_key.as_deref() == Some(&key))
            });
            let counter = match op.get("versions").and_then(|v| v.get("gameData")) {
                Some(Value::Int(n)) => n.parse::<i64>().ok(),
                _ => None,
            };
            match text_of(op.get("op")).as_str() {
                "create" => {
                    if let Some(at) = self.orders.iter().position(|o| o.client_key.as_deref() == Some(&key)) {
                        let mut refused = failure(index, "rejected", "ALREADY_EXISTS");
                        if let Value::Object(pairs) = &mut refused {
                            pairs.push(("id".into(), Value::str(&self.orders[at].id)));
                        }
                        report.push(refused);
                        continue;
                    }
                    let unit = text_of(op.get("orgUnit").and_then(|u| u.get("id")));
                    if !self.units.iter().any(|(u, _)| *u == unit) {
                        report.push(failure(index, "rejected", "NOT_A_MEMBER"));
                        continue;
                    }
                    let place = text_of(op.get("location").and_then(|l| l.get("name")));
                    let Some(refinery) = REFINERIES.iter().find(|r| lower(r) == lower(&place)) else {
                        let known = self.places.iter().any(|p| lower(p) == lower(&place));
                        report.push(if known {
                            failure(index, "rejected", "NOT_A_REFINERY")
                        } else {
                            failure(index, "unmatched", "UNMATCHED")
                        });
                        continue;
                    };
                    let mut goods = Vec::new();
                    let mut reason = "";
                    for good in match op.get("goods") {
                        Some(Value::Array(goods)) => goods.clone(),
                        _ => Vec::new(),
                    } {
                        let bt = text_of(good.get("input").and_then(|i| i.get("bt")));
                        match self.materials.iter().find(|(b, _, item)| *b == bt && !item) {
                            Some((_, name, _)) if name.ends_with(" (Ore)") || name.ends_with(" (Raw)") => {
                                let output = name.trim_end_matches(" (Ore)").trim_end_matches(" (Raw)").to_string();
                                let amount = amount_of(good.get("outputQuantity")) as i64;
                                let quality = amount_of(good.get("quality")) as i64;
                                goods.push((output, amount, quality));
                            }
                            Some(_) => reason = "MATERIAL_NOT_RAW",
                            None => reason = "UNMATCHED",
                        }
                    }
                    if goods.is_empty() && reason.is_empty() {
                        reason = "GOODS_REQUIRED";
                    }
                    if !reason.is_empty() {
                        report.push(failure(index, "rejected", reason));
                        continue;
                    }
                    let new_id = format!("ro-{}", self.next);
                    self.next += 1;
                    let status = text_of(op.get("status"));
                    self.orders.push(RefineryOrder {
                        id: new_id.clone(),
                        client_key: Some(key),
                        status: if status.is_empty() { "OPEN".into() } else { status },
                        place: (*refinery).to_string(),
                        ends_at: text_of(op.get("endsAt")),
                        goods,
                        game_data: 0,
                        picked_up: false,
                    });
                    let revision = self.bump();
                    self.order_log.push(Log { revision, key: new_id, gone_by: None });
                    applied += 1;
                }
                kind @ ("update" | "pickup") => {
                    let Some(at) = found else {
                        report.push(failure(index, "rejected", "ORDER_UNKNOWN"));
                        continue;
                    };
                    if kind == "pickup" && op.get("lines").is_some() {
                        report.push(failure(index, "rejected", "SCOPE_MISSING"));
                        continue;
                    }
                    let order = &mut self.orders[at];
                    if kind == "pickup" && order.status == "COMPLETED" {
                        unchanged += 1;
                        continue;
                    }
                    if kind == "pickup" && order.status == "CANCELED" {
                        report.push(failure(index, "rejected", "ORDER_CANCELED"));
                        continue;
                    }
                    if counter != Some(order.game_data) {
                        report.push(failure(index, "rejected", "VERSION_CONFLICT"));
                        continue;
                    }
                    if kind == "pickup" {
                        order.status = "COMPLETED".into();
                        order.picked_up = true;
                    } else if let Some(ends) = op.get("endsAt").and_then(Value::as_str) {
                        order.ends_at = ends.into();
                    }
                    order.game_data += 1;
                    let order_id = order.id.clone();
                    let revision = self.bump();
                    self.order_log.push(Log { revision, key: order_id, gone_by: None });
                    applied += 1;
                }
                _ => report.push(failure(index, "rejected", "UNKNOWN_OP")),
            }
        }
        results(applied, unchanged, report, self.revision)
    }
}

impl ExchangeDouble {
    /// Answers a request given by its whole address, under the made-up issuer
    /// `issuer` and the API `api`. The transport of the tests and a server on
    /// `127.0.0.1` both go through here.
    pub fn respond(&self, method: &str, url: &str, body: Option<&[u8]>, issuer: &str, api: &str) -> Response {
        let (path, query) = url.split_once('?').unwrap_or((url, ""));
        let query = query_of(query);
        let body =
            body.and_then(|b| std::str::from_utf8(b).ok()).and_then(|t| jsonio::loads(t).ok()).unwrap_or(Value::Null);
        {
            let shown = path.strip_prefix(api).or_else(|| path.strip_prefix(issuer)).unwrap_or(path);
            let line = if query.is_empty() {
                format!("{method} {shown}")
            } else {
                let pairs: Vec<String> = query.iter().map(|(k, v)| format!("{k}={v}")).collect();
                format!("{method} {shown}?{}", pairs.join("&"))
            };
            self.world().seen.push(line);
        }
        if path == format!("{issuer}/.well-known/openid-configuration") {
            return answer(
                200,
                object(vec![
                    ("issuer", Value::str(issuer)),
                    ("device_authorization_endpoint", Value::str(format!("{issuer}/device"))),
                    ("token_endpoint", Value::str(format!("{issuer}/token"))),
                ]),
            );
        }
        if path == format!("{issuer}/token") {
            let fault = {
                let mut world = self.world();
                match world.token_faults.iter().position(|f| f.2 > 0) {
                    Some(at) => {
                        world.token_faults[at].2 -= 1;
                        Some(world.token_faults[at].clone())
                    }
                    None => None,
                }
            };
            if let Some((status, error, _)) = fault {
                let body =
                    if error.is_empty() { object(Vec::new()) } else { object(vec![("error", Value::str(error))]) };
                return answer(status, body);
            }
            let rights = self.world().rights.clone();
            let scope = format!("exchange.connect {} offline_access", rights.join(" "));
            return answer(
                200,
                object(vec![
                    ("token_type", Value::str("DPoP")),
                    ("access_token", Value::str("doppel-access")),
                    ("refresh_token", Value::str("doppel-refresh")),
                    ("expires_in", Value::int(300)),
                    ("scope", Value::str(scope)),
                ]),
            );
        }
        let route = path.strip_prefix(api).unwrap_or("?");
        self.handle(method, route, &query, &body)
    }
}

impl Transport for ExchangeDouble {
    /// A change set with an `Idempotency-Key` seen before gets the answer of that time again, as the Basetool keeps
    /// it for 24 hours; nothing is applied a second time.
    fn send(&self, request: &Request) -> Result<Response, ApiError> {
        let key = crate::basetool::proof::header(&request.headers, "Idempotency-Key").map(str::to_string);
        // Every write of the API carries a key; a key comes back only with the body it was used for.
        let route = request.url.strip_prefix(API).unwrap_or_default();
        if request.method == "POST" && key.is_none() && (route.ends_with("/changes") || route.starts_with("/bank/")) {
            return Ok(problem(400, "IDEMPOTENCY_KEY_MISSING"));
        }
        if let Some(key) = &key {
            let known = self.world().replays.iter().find(|(k, _, _)| k == key).map(|(_, b, r)| (b.clone(), r.clone()));
            if let Some((body, known)) = known {
                if body != request.body {
                    return Ok(problem(422, "IDEMPOTENCY_KEY_REUSED"));
                }
                self.world().seen.push(format!("{} {} (replay)", request.method, request.url));
                return Ok(known);
            }
        }
        let response = self.respond(&request.method, &request.url, request.body.as_deref(), ISSUER, API);
        let mut world = self.world();
        if let Some(key) = key
            && (200..300).contains(&response.status)
        {
            world.replays.push((key, request.body.clone(), response.clone()));
            let route = request.url.strip_prefix(API).unwrap_or_default().to_string();
            if let Some(at) = world.lose_answers.iter().position(|r| *r == route) {
                world.lose_answers.remove(at);
                return Ok(problem(503, "SERVICE_UNAVAILABLE"));
            }
        }
        Ok(response)
    }
}

/// The `ops` of a body as maps (for the tests).
pub fn ops_map(body: &Value) -> Vec<Map> {
    ops_of(body).into_iter().filter_map(|v| if let Value::Object(m) = v { Some(m) } else { None }).collect()
}
