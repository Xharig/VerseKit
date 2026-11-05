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

//! What the Game.log says about the stock: the purchases and sales at a kiosk
//! and the moves of the inventory.
//!
//! ⚠ The log has no stock, only events. A purchase is certain: a request with
//! amount, class and total price, and an answer that says it went through. A
//! sale at a kiosk is the same request with `Sell` and an answer of the type
//! `Selling`; a sale of goods at a commodity terminal has no answer line and is
//! not read. A
//! move only says where a piece went; the stock before it is not in the log,
//! so the moves are a hint, never a sum.
//!
//! ⚠ A move is read from its `Queued` line alone: the `New` line is mostly
//! missing, often has source and target swapped and no usable class.
//! `Drop` stands in `Queued` as `Interaction`. The request number is a
//! counter that runs twice in a log (a second one for the queries starts at 1
//! again), so a result belongs to the move with the same number **and** kind,
//! never to the number alone.
//!
//! ⚠ Nothing that names the player goes into an event: no handle, no player
//! number, no `playerId`, `shopId` or `kioskId`. A place is a bare number.
//!
//! The events of every log are kept per session in `lagerjournal.json`, the
//! same way `play_stats` keeps its sessions: the key is the start of the
//! session, every log is read once (name and size), a log that grows is read
//! again and replaces its own session.

use std::collections::{HashMap, VecDeque};
use std::fs;
use std::sync::LazyLock;

use regex::Regex;

pub mod names;
pub mod spending;
pub mod suggestions;

use crate::jsonio::{self, Map, Value, map_set};
use crate::location;
use crate::logsource::{LogSource, counts_own};
use crate::logtime::{Facts, candidate_lines, seconds, time_of};
use crate::mission_log::get;
use crate::ordered::parallel_map;
use crate::paths::Paths;
use crate::playtime::{number, write_beside};
use crate::pypath::Flavor;
use crate::text::decode_replace;

pub const FILE: &str = "lagerjournal.json";
/// 2 holds the sales (`verkaeufe`); an older journal reads every log once more.
pub const FORMAT: i64 = 2;
/// The setting that switches the reading of the stock lines on (on by default).
pub const SETTING: &str = "lager_aus_log";

/// What a move does with a piece.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    Store,
    Interaction,
    Move,
    Split,
    Stack,
    StackAll,
}

impl Kind {
    pub const ALL: [Kind; 6] = [Kind::Store, Kind::Interaction, Kind::Move, Kind::Split, Kind::Stack, Kind::StackAll];

    /// The name the log gives it.
    pub fn name(self) -> &'static str {
        match self {
            Kind::Store => "Store",
            Kind::Interaction => "Interaction",
            Kind::Move => "Move",
            Kind::Split => "Split",
            Kind::Stack => "Stack",
            Kind::StackAll => "StackAll",
        }
    }

    /// The kind of a request type of the log; `None` for the queries and everything that moves nothing.
    pub fn from_log(name: &str) -> Option<Kind> {
        match name {
            "Drop" => Some(Kind::Interaction),
            _ => Kind::ALL.into_iter().find(|kind| kind.name() == name),
        }
    }
}

/// One end of a move.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Endpoint {
    /// `INVALID`: the piece comes from or goes to nowhere in the inventory (taken in the hand, put in from outside).
    Nowhere,
    /// A container of the player: backpack, crate, ship hold.
    Container,
    /// The stock at a place, with the number of the place.
    Location(String),
    Other,
}

impl Endpoint {
    fn parse(text: &str) -> Endpoint {
        if text == "INVALID" {
            return Endpoint::Nowhere;
        }
        match text.split(':').collect::<Vec<_>>().as_slice() {
            [_, "Container", _] => Endpoint::Container,
            [_, "Location", number] if !number.is_empty() => Endpoint::Location((*number).to_string()),
            _ => Endpoint::Other,
        }
    }

    /// The number of the place, when this end is the stock of a place.
    pub fn place(&self) -> Option<&str> {
        match self {
            Endpoint::Location(number) => Some(number),
            _ => None,
        }
    }
}

/// A purchase or a sale the shop confirmed.
#[derive(Clone, Debug, PartialEq)]
pub struct Deal {
    /// Seconds since 1970 (UTC), the stamp of the request.
    pub time: i64,
    /// The class of the item (`behr_lmg_ballistic_01_mag`), not the name shown in the game.
    pub class: String,
    /// The entity GUID the shop lists the item under.
    pub guid: String,
    pub quantity: u32,
    /// The price of the request for all pieces together: paid for a purchase, received for a sale.
    pub price_total: f64,
    /// The number of the landing place at the request; `None` while the player was in flight.
    pub place: Option<String>,
    /// The name of the shop (`SCShop_CargoOfficePyroStation`).
    pub shop: String,
}

/// A purchase the shop confirmed.
pub type Purchase = Deal;
/// A sale the shop confirmed.
pub type Sale = Deal;

impl Deal {
    /// The price of one piece; a fraction when the total does not divide.
    pub fn unit_price(&self) -> f64 {
        self.price_total / f64::from(self.quantity.max(1))
    }
}

/// A move of the inventory, from its `Queued` line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Movement {
    pub time: i64,
    pub kind: Kind,
    /// The class of the piece; empty when the line does not name one.
    pub class: String,
    pub from: Endpoint,
    pub to: Endpoint,
    /// The amount of the source as the line gives it (the size of the moved stack, not proven to be a count).
    pub amount: i64,
    /// `Some(true)` for `Succeed`, `Some(false)` for any other result, `None` while no result line was seen.
    pub ok: Option<bool>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    Purchase(Purchase),
    Sale(Sale),
    Movement(Movement),
}

static QUEUED: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(concat!(
        r"<(?:InventoryManagementRequest|Inventory Mgmt Request Queued)> Queued Request\[(\d+)\] Type\[(\w+)\] ",
        r"for '([^']*)' \[(\d+)\] Source Inventory\[([^\]]*)\] Target Inventory\[([^\]]*)\]\. ",
        r"Source\[([^\]]*)\] amount\[(-?\d+)\].*?Item\[([^\]]*)\] action\["
    ))
    .expect("pattern")
});
static COMPLETE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"<Player Inventory Request Complete> Request\[(\d+)\] for '([^']*)' \[(\d+)\] Type\[(\w+)\] Result\[(\w+)\]",
    )
    .expect("pattern")
});
static BUY: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(concat!(
        r"<CEntityComponent(ShopUI|Shopping)Provider::Send(?:Shop|StandardItem)BuyRequest> Sending SShopBuyRequest - ",
        r"playerId\[(\d+)\] shopId\[[^\]]*\] shopName\[([^\]]*)\] kioskId\[[^\]]*\] client_price\[([^\]]*)\] ",
        r"itemClassGUID\[([^\]]*)\] itemName\[([^\]]*)\] quantity\[(\d+)\]"
    ))
    .expect("pattern")
});
static SELL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(concat!(
        r"<CEntityComponentShopUIProvider::SendShopSellRequest> Sending SShopSellRequest - ",
        r"playerId\[(\d+)\] shopId\[[^\]]*\] shopName\[([^\]]*)\] kioskId\[[^\]]*\] client_price\[([^\]]*)\] ",
        r"itemClassGUID\[([^\]]*)\] itemName\[([^\]]*)\] quantity\[(\d+)\]"
    ))
    .expect("pattern")
});
static ANSWER: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"<CEntityComponent(ShopUI|Shopping)Provider::RmShopFlowResponse>.*?result\[(\w+)\]").expect("pattern")
});
static ANSWER_TYPE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"type\[(\w+)\]").expect("pattern"));
/// The number at the end of the name of a single entity: `Fuse_subItem_standard_468366566217` → `Fuse_subItem_standard`.
static SERIAL: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"_\d{6,}$").expect("pattern"));

/// The lines worth a look; every other line cannot change anything.
static MARKERS: LazyLock<Regex> = LazyLock::new(|| {
    let literals = [
        "Queued Request[",
        "<Player Inventory Request Complete>",
        "SShopBuyRequest",
        "SShopSellRequest",
        "RmShopFlowResponse",
        "<Update Inventory Location>",
    ];
    let escaped: Vec<String> = literals.iter().map(|l| regex::escape(l)).collect();
    Regex::new(&escaped.join("|")).expect("built from escaped parts")
});

/// The class a move names: the item for a store, the source for every other kind.
fn class_of(kind: Kind, source: &str, item: &str) -> String {
    let raw = if kind == Kind::Store { item } else { source };
    match raw {
        "" | "NULL" | "NONE" => String::new(),
        _ => SERIAL.replace(raw, "").into_owned(),
    }
}

/// A buy or sell request waiting for its answer.
struct Pending {
    deal: Deal,
    /// Only to tell whose request it is; never leaves this module.
    player: String,
}

#[derive(Default)]
struct Reading {
    events: Vec<(Event, Option<String>)>,
    /// The move of each (request number, kind) that has no result yet, as an index into `events`.
    open: HashMap<(String, Kind), usize>,
    /// Player number → handle in lower case, from the lines that carry both.
    handles: HashMap<String, String>,
    /// The requests waiting for an answer, one queue per way of buying.
    waiting: [VecDeque<Pending>; 2],
    /// The sell requests waiting for an answer.
    selling: VecDeque<Pending>,
    landing: Option<String>,
}

impl Reading {
    fn line(&mut self, line: &str, own: &dyn Fn(&str) -> bool) {
        if line.contains("Queued Request[") {
            self.queued(line, own);
        } else if line.contains("<Player Inventory Request Complete>") {
            self.complete(line, own);
        } else if line.contains("SShopBuyRequest") {
            self.buy(line);
        } else if line.contains("SShopSellRequest") {
            self.sell(line);
        } else if line.contains("RmShopFlowResponse") {
            self.answer(line);
        } else if line.contains("<Update Inventory Location>")
            && let Some((_, now)) = location::change_in(line)
        {
            self.landing = Some(now).filter(|n| n != location::NOWHERE);
        }
    }

    fn queued(&mut self, line: &str, own: &dyn Fn(&str) -> bool) {
        let Some(c) = QUEUED.captures(line) else { return };
        self.handles.insert(c[4].to_string(), c[3].to_lowercase());
        let Some(kind) = Kind::from_log(&c[2]) else { return };
        let Some(time) = seconds(time_of(line)) else { return };
        if !own(&c[3]) {
            return;
        }
        let movement = Movement {
            time,
            kind,
            class: class_of(kind, &c[7], &c[9]),
            from: Endpoint::parse(&c[5]),
            to: Endpoint::parse(&c[6]),
            amount: c[8].parse().unwrap_or(0),
            ok: None,
        };
        let key = (c[1].to_string(), kind);
        // The same line twice (both marks of the log naming one request) is one move.
        if let Some(&index) = self.open.get(&key)
            && matches!(&self.events[index].0, Event::Movement(earlier) if earlier == &movement)
        {
            return;
        }
        self.open.insert(key, self.events.len());
        self.events.push((Event::Movement(movement), None));
    }

    fn complete(&mut self, line: &str, own: &dyn Fn(&str) -> bool) {
        let Some(c) = COMPLETE.captures(line) else { return };
        self.handles.insert(c[3].to_string(), c[2].to_lowercase());
        let Some(kind) = Kind::from_log(&c[4]) else { return };
        if !own(&c[2]) {
            return;
        }
        if let Some(index) = self.open.remove(&(c[1].to_string(), kind))
            && let Event::Movement(movement) = &mut self.events[index].0
        {
            movement.ok = Some(&c[5] == "Succeed");
        }
    }

    /// The request of a line from the captures player, shop, price, GUID, class and amount, in this order from
    /// `first` on; `None` for a request of no piece or a price that is no number.
    fn request(&self, line: &str, c: &regex::Captures<'_>, first: usize) -> Option<Pending> {
        let time = seconds(time_of(line))?;
        let field = |n: usize| &c[first + n];
        let (Ok(price_total), Ok(quantity)) = (field(2).parse::<f64>(), field(5).parse::<u32>()) else { return None };
        if quantity == 0 || !price_total.is_finite() || price_total < 0.0 {
            return None;
        }
        let deal = Deal {
            time,
            class: field(4).to_string(),
            guid: field(3).to_string(),
            quantity,
            price_total,
            place: self.landing.clone(),
            shop: field(1).to_string(),
        };
        Some(Pending { deal, player: field(0).to_string() })
    }

    fn buy(&mut self, line: &str) {
        let Some(c) = BUY.captures(line) else { return };
        let Some(pending) = self.request(line, &c, 2) else { return };
        let way = usize::from(&c[1] == "Shopping");
        self.waiting[way].push_back(pending);
    }

    fn sell(&mut self, line: &str) {
        let Some(c) = SELL.captures(line) else { return };
        if let Some(pending) = self.request(line, &c, 1) {
            self.selling.push_back(pending);
        }
    }

    fn answer(&mut self, line: &str) {
        let Some(c) = ANSWER.captures(line) else { return };
        let way = usize::from(&c[1] == "Shopping");
        let (queue, sale) = match ANSWER_TYPE.captures(line).as_ref().map(|t| &t[1]) {
            None | Some("Buying") => (&mut self.waiting[way], false),
            // Only the kiosk way is known to sell.
            Some("Selling") if way == 0 => (&mut self.selling, true),
            // A flow that is neither takes no request with it.
            Some(_) => return,
        };
        let Some(done) = queue.pop_front() else { return };
        if &c[2] == "Success" {
            let event = if sale { Event::Sale(done.deal) } else { Event::Purchase(done.deal) };
            self.events.push((event, Some(done.player)));
        }
    }
}

/// The events of one log's text, in the order of the log: every move the player's inventory queued (with its
/// result when the log has one) and every purchase and sale the shop confirmed.
///
/// `account` is the player of this log; only its own lines count. A deal names the player by number only,
/// so it counts unless that number is known to belong to another handle. Without an account everyone counts.
pub fn parse_text(text: &str, account: Option<&str>) -> Vec<Event> {
    let account = account.filter(|a| !a.is_empty()).map(str::to_lowercase);
    let own = |handle: &str| account.as_deref().is_none_or(|a| handle.to_lowercase() == a);
    let mut reading = Reading::default();
    for line in candidate_lines(text, &MARKERS) {
        reading.line(line, &own);
    }
    let Reading { events, handles, .. } = reading;
    events
        .into_iter()
        .filter(|(_, player)| player.as_ref().and_then(|number| handles.get(number)).is_none_or(|handle| own(handle)))
        .map(|(event, _)| event)
        .collect()
}

impl Endpoint {
    fn to_text(&self) -> String {
        match self {
            Endpoint::Nowhere => "kein".into(),
            Endpoint::Container => "behaelter".into(),
            Endpoint::Location(number) => format!("ort:{number}"),
            Endpoint::Other => "anderes".into(),
        }
    }

    fn from_text(text: &str) -> Endpoint {
        match text {
            "kein" => Endpoint::Nowhere,
            "behaelter" => Endpoint::Container,
            _ => match text.strip_prefix("ort:") {
                Some(number) if !number.is_empty() => Endpoint::Location(number.to_string()),
                _ => Endpoint::Other,
            },
        }
    }
}

fn int_of(value: &Value, key: &str) -> Option<i64> {
    match value.get(key)? {
        Value::Int(text) => text.parse().ok(),
        _ => None,
    }
}

fn text_of<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key)?.as_str()
}

impl Deal {
    /// The entry of the journal: `zeit`, `klasse`, `guid`, `menge`, `preis` (the total), `ort` (empty in flight)
    /// and `laden`.
    pub fn to_value(&self) -> Value {
        Value::Object(vec![
            ("zeit".into(), Value::int(self.time)),
            ("klasse".into(), Value::str(&self.class)),
            ("guid".into(), Value::str(&self.guid)),
            ("menge".into(), Value::int(i64::from(self.quantity))),
            ("preis".into(), Value::Float(self.price_total)),
            ("ort".into(), Value::str(self.place.clone().unwrap_or_default())),
            ("laden".into(), Value::str(&self.shop)),
        ])
    }

    /// The deal of an entry of the journal; `None` when a field is missing.
    pub fn from_value(value: &Value) -> Option<Deal> {
        let price_total = match value.get("preis")? {
            Value::Float(price) => *price,
            Value::Int(text) => text.parse().ok()?,
            _ => return None,
        };
        Some(Deal {
            time: int_of(value, "zeit")?,
            class: text_of(value, "klasse")?.to_string(),
            guid: text_of(value, "guid")?.to_string(),
            quantity: u32::try_from(int_of(value, "menge")?).ok()?,
            price_total,
            place: Some(text_of(value, "ort")?.to_string()).filter(|p| !p.is_empty()),
            shop: text_of(value, "laden")?.to_string(),
        })
    }
}

impl Movement {
    /// The entry of the journal: `zeit`, `art`, `klasse`, `quelle`, `ziel`, `menge` and `ok` (`null` without a
    /// result line).
    pub fn to_value(&self) -> Value {
        Value::Object(vec![
            ("zeit".into(), Value::int(self.time)),
            ("art".into(), Value::str(self.kind.name())),
            ("klasse".into(), Value::str(&self.class)),
            ("quelle".into(), Value::str(self.from.to_text())),
            ("ziel".into(), Value::str(self.to.to_text())),
            ("menge".into(), Value::int(self.amount)),
            ("ok".into(), self.ok.map_or(Value::Null, Value::Bool)),
        ])
    }

    /// The move of an entry of the journal; `None` when a field is missing.
    pub fn from_value(value: &Value) -> Option<Movement> {
        let ok = match value.get("ok")? {
            Value::Bool(b) => Some(*b),
            Value::Null => None,
            _ => return None,
        };
        Some(Movement {
            time: int_of(value, "zeit")?,
            kind: Kind::from_log(text_of(value, "art")?)?,
            class: text_of(value, "klasse")?.to_string(),
            from: Endpoint::from_text(text_of(value, "quelle")?),
            to: Endpoint::from_text(text_of(value, "ziel")?),
            amount: int_of(value, "menge")?,
            ok,
        })
    }
}

/// The session entry of one log's text and its events: `von`, `bis` (seconds, UTC), `kaeufe`, `verkaeufe`,
/// `bewegungen`.
/// `None` without an event, or when the stamps make no session.
pub fn session_entry(text: &str, events: &[Event]) -> Option<Map> {
    if events.is_empty() {
        return None;
    }
    let facts = Facts::of(text);
    let start = seconds(facts.first.as_deref().unwrap_or("")).filter(|s| *s != 0)?;
    let end = seconds(facts.last.as_deref().unwrap_or("")).filter(|s| *s != 0)?;
    if end < start {
        return None;
    }
    let mut bought = Vec::new();
    let mut sold = Vec::new();
    let mut moved = Vec::new();
    for event in events {
        match event {
            Event::Purchase(purchase) => bought.push(purchase.to_value()),
            Event::Sale(sale) => sold.push(sale.to_value()),
            Event::Movement(movement) => moved.push(movement.to_value()),
        }
    }
    Some(vec![
        ("von".into(), Value::int(start)),
        ("bis".into(), Value::int(end)),
        ("kaeufe".into(), Value::Array(bought)),
        ("verkaeufe".into(), Value::Array(sold)),
        ("bewegungen".into(), Value::Array(moved)),
    ])
}

/// The session entry of one log's text; see [`parse_text`] for `account`.
pub fn read_log_text(text: &str, account: Option<&str>) -> Option<Map> {
    session_entry(text, &parse_text(text, account))
}

/// [`read_log_text`] of a log file; an unreadable one has none.
pub fn read_log(path: &str, account: Option<&str>) -> Option<Map> {
    let bytes = fs::read(path).ok()?;
    read_log_text(&decode_replace(&bytes), account)
}

/// One session of the journal.
#[derive(Clone, Debug, PartialEq)]
pub struct Session {
    /// Seconds since 1970 (UTC) of the first stamp of the log.
    pub start: i64,
    pub purchases: Vec<Purchase>,
    pub sales: Vec<Sale>,
    pub movements: Vec<Movement>,
}

/// The sessions of the journal, oldest first.
pub fn sessions(data: &Map) -> Vec<Session> {
    let Some(Value::Object(entries)) = get(data, "sitzungen") else { return Vec::new() };
    let mut found: Vec<Session> = entries
        .iter()
        .filter_map(|(_, entry)| {
            let list = |key: &str| match entry.get(key) {
                Some(Value::Array(items)) => items.clone(),
                _ => Vec::new(),
            };
            Some(Session {
                start: int_of(entry, "von")?,
                purchases: list("kaeufe").iter().filter_map(Deal::from_value).collect(),
                sales: list("verkaeufe").iter().filter_map(Deal::from_value).collect(),
                movements: list("bewegungen").iter().filter_map(Movement::from_value).collect(),
            })
        })
        .collect();
    found.sort_by_key(|session| session.start);
    found
}

/// Is the reading of the stock lines switched on? On unless the setting says no.
pub fn enabled(paths: &Paths) -> bool {
    paths.setting_bool(SETTING, true)
}

/// Switches the reading of the stock lines on or off; false when the setting could not be written.
pub fn set_enabled(paths: &Paths, on: bool) -> bool {
    paths.set_setting(SETTING, Value::Bool(on))
}

/// `{'format':…, 'sitzungen': {von: {…}}, 'gelesen': {name: size}}`.
///
/// ⚠ An older format keeps its sessions but loses the read position, so every log still there is read once
/// with the new rules.
pub fn load(paths: &Paths) -> Map {
    let valid = |d: &Value| matches!(d, Value::Object(_));
    match jsonio::load_json(&paths.app_file(FILE), Some(&valid), "stock_journal.load") {
        Some(Value::Object(mut map)) if matches!(get(&map, "sitzungen"), Some(Value::Object(_))) => {
            if number(get(&map, "format")) != Some(FORMAT as f64) {
                map_set(&mut map, "gelesen", Value::Object(Vec::new()));
            }
            if get(&map, "gelesen").is_none() {
                map.push(("gelesen".into(), Value::Object(Vec::new())));
            }
            map
        }
        _ => vec![
            ("format".into(), Value::int(FORMAT)),
            ("sitzungen".into(), Value::Object(Vec::new())),
            ("gelesen".into(), Value::Object(Vec::new())),
        ],
    }
}

/// Writes first beside, then renames; reports a failure.
pub fn save(paths: &Paths, data: &mut Map) -> bool {
    write_beside(paths, FILE, data, FORMAT, "stock_journal.save")
}

/// Reads the logs that are new or have grown and writes their sessions into `lagerjournal.json`; returns the
/// number of newly recorded sessions. Does nothing while the setting is off.
///
/// ⚠ Its own read position (name and size). ⚠ The key is the start of the session, not the file name: the
/// running `Game.log` is renamed at the next start and would count a second time. ⚠ A log of another account
/// than the own one is marked as read and records nothing.
pub fn catch_up(src: &LogSource<'_>, files: &[String]) -> usize {
    let paths = src.paths;
    if !enabled(paths) {
        return 0;
    }
    let known = match get(&load(paths), "gelesen") {
        Some(Value::Object(map)) => map.clone(),
        _ => Vec::new(),
    };
    let own = src.own_identity(Some(files));
    let mut marks: Vec<(String, i64)> = Vec::new();
    let mut jobs: Vec<(&String, Option<String>)> = Vec::new();
    for path in files {
        let Ok(meta) = fs::metadata(path) else { continue };
        let size = i64::try_from(meta.len()).unwrap_or(i64::MAX);
        let name = Flavor::native().basename(path);
        if number(get(&known, &name)) == Some(size as f64) {
            continue;
        }
        marks.push((name, size));
        let identity = src.identity_of_file(path);
        if counts_own(own.as_ref(), &identity) {
            jobs.push((path, identity.handle));
        }
    }
    let entries: Vec<Map> =
        parallel_map(&jobs, |(path, account)| read_log(path, account.as_deref())).into_iter().flatten().collect();
    if marks.is_empty() {
        return 0;
    }
    // ⚠ The reading above takes long and runs without the lock; what is written is loaded anew under it.
    let _guard = jsonio::lock_saves();
    let mut data = load(paths);
    let mut recorded: Map = match get(&data, "sitzungen") {
        Some(Value::Object(map)) => map.clone(),
        _ => Vec::new(),
    };
    let mut read: Map = match get(&data, "gelesen") {
        Some(Value::Object(map)) => map.clone(),
        _ => Vec::new(),
    };
    for (name, size) in &marks {
        map_set(&mut read, name, Value::int(*size));
    }
    let mut added = 0;
    for entry in entries {
        let key = get(&entry, "von").map_or_else(String::new, |v| jsonio::dumps(v, None, false));
        if get(&recorded, &key).is_none() {
            added += 1;
        }
        map_set(&mut recorded, &key, Value::Object(entry));
    }
    map_set(&mut data, "sitzungen", Value::Object(recorded));
    map_set(&mut data, "gelesen", Value::Object(read));
    save(paths, &mut data);
    added
}

/// "Read everything anew": every log there is is read once more. ⚠ Only the read position is dropped, not the
/// sessions: what has long since vanished from the logs keeps its entry.
pub fn rescan(src: &LogSource<'_>, files: &[String]) -> usize {
    {
        let _guard = jsonio::lock_saves();
        let mut data = load(src.paths);
        map_set(&mut data, "gelesen", Value::Object(Vec::new()));
        save(src.paths, &mut data);
    }
    catch_up(src, files)
}

#[cfg(test)]
mod tests {
    use super::*;

    const WHO: &str = "Spieler";

    /// A log line with a stamp; `n` counts the seconds.
    fn at(n: u32, body: &str) -> String {
        format!("<2026-10-01T10:{:02}:{:02}.000Z> {body}\n", n / 60, n % 60)
    }

    fn queued(n: u32, kind: &str, from: &str, to: &str, source: &str, amount: u32, item: &str) -> String {
        format!(
            "[Notice] <Inventory Mgmt Request Queued> Queued Request[{n}] Type[{kind}] for '{WHO}' [4711] \
             Source Inventory[{from}] Target Inventory[{to}]. Source[{source}] amount[{amount}] rank[x]. \
             Target[NULL] amount[0] rank[]. Item[{item}] action[None]. RequestInProgress[0] CurrentProcess[] [Team][Inventory]"
        )
    }

    fn done(n: u32, kind: &str, result: &str) -> String {
        format!(
            "[Notice] <Player Inventory Request Complete> Request[{n}] for '{WHO}' [4711] Type[{kind}] \
             Result[{result}] Item[0] CanLockQueue[Yes]. [Team][Inventory]"
        )
    }

    fn buy_way_one(price: &str, class: &str, quantity: u32) -> String {
        format!(
            "[Notice] <CEntityComponentShopUIProvider::SendShopBuyRequest> Sending SShopBuyRequest - \
             playerId[4711] shopId[99] shopName[SCShop_Kiosk] kioskId[77] client_price[{price}] \
             itemClassGUID[72b91153-5a3e-4d71-af5c-f6c57ea2891a] itemName[{class}] quantity[{quantity}]  [Team][Shops]"
        )
    }

    const ANSWER_ONE: &str = "[Notice] <CEntityComponentShopUIProvider::RmShopFlowResponse> Received ShopFlowResponse - \
         playerId[4711] shopId[99] shopName[SCShop_Kiosk] kioskId[77] kioskState[BuyRequestProcessing] \
         result[Success] type[Buying] [Team][Shops]";

    /// A sell request at a kiosk, built after the form of the game (the numbers are made up).
    fn sell_line(price: &str, class: &str, quantity: u32) -> String {
        format!(
            "[Notice] <CEntityComponentShopUIProvider::SendShopSellRequest> Sending SShopSellRequest - \
             playerId[4711] shopId[99] shopName[SCShop_Pharmacy] kioskId[77] client_price[{price}] \
             itemClassGUID[0b6c7d2e-1f3a-4c5b-9d8e-7a6b5c4d3e2f] itemName[{class}] quantity[{quantity}]  [Team][Shops][UI]"
        )
    }

    const ANSWER_SOLD: &str = "[Notice] <CEntityComponentShopUIProvider::RmShopFlowResponse> Received ShopFlowResponse - \
         playerId[4711] shopId[99] shopName[SCShop_Pharmacy] kioskId[77] kioskState[SellRequestProcessing] \
         result[Success] type[Selling] [Team][Shops][UI]";

    /// A sell request of goods at a commodity terminal; the game writes no answer for it.
    const COMMODITY_SOLD: &str = "[Notice] <CEntityComponentCommodityUIProvider::SendCommoditySellRequest> Sending \
         SShopCommoditySellRequest - playerId[4711] shopId[99] shopName[SCShop_CommEx] kioskId[77] amount[15710.000000] \
         resourceGUID[1a2b3c4d-0000-4000-8000-000000000001] autoLoading[0] quantity[2] transactionMode[ResourceContainer] \
         Cargo Box Data:  [boxSize[1] | unitAmount[2]] [Team][Shops][UI]";

    fn movements(events: &[Event]) -> Vec<&Movement> {
        events
            .iter()
            .filter_map(|e| match e {
                Event::Movement(m) => Some(m),
                _ => None,
            })
            .collect()
    }

    fn purchases(events: &[Event]) -> Vec<&Purchase> {
        events
            .iter()
            .filter_map(|e| match e {
                Event::Purchase(p) => Some(p),
                _ => None,
            })
            .collect()
    }

    fn sales(events: &[Event]) -> Vec<&Sale> {
        events
            .iter()
            .filter_map(|e| match e {
                Event::Sale(s) => Some(s),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn a_sale_at_the_kiosk_is_read_with_its_answer_and_its_place() {
        let landing = "[Notice] <Update Inventory Location> Player [Spieler] is changing location. Landing [0] -> [1105]. Location [0] -> [1105]. Pending [0]";
        let text = [at(1, landing), at(2, &sell_line("62.000000", "nvtc_ubarrel_lasr_s1", 1)), at(3, ANSWER_SOLD)].concat();
        let events = parse_text(&text, Some(WHO));
        assert!(purchases(&events).is_empty(), "a sale is no purchase");
        let sold = sales(&events);
        assert_eq!(sold.len(), 1);
        let sale = sold[0];
        assert_eq!((sale.class.as_str(), sale.quantity, sale.place.as_deref()), ("nvtc_ubarrel_lasr_s1", 1, Some("1105")));
        assert!((sale.price_total - 62.0).abs() < f64::EPSILON);
        assert_eq!(sale.shop, "SCShop_Pharmacy");
        assert_eq!(sale.guid, "0b6c7d2e-1f3a-4c5b-9d8e-7a6b5c4d3e2f");
        // The price of the request is the total, like for a purchase.
        let two = [at(1, &sell_line("124.000000", "mag", 2)), at(2, ANSWER_SOLD)].concat();
        assert!((sales(&parse_text(&two, Some(WHO)))[0].unit_price() - 62.0).abs() < f64::EPSILON);
    }

    #[test]
    fn a_sale_and_a_purchase_take_their_own_answers() {
        // Buy, then sell, then the answer of the sale comes before the one of the purchase.
        let text = [
            at(1, &buy_way_one("10.000000", "bought", 1)),
            at(2, &sell_line("20.000000", "sold", 1)),
            at(3, ANSWER_SOLD),
            at(4, ANSWER_ONE),
        ]
        .concat();
        let events = parse_text(&text, Some(WHO));
        let bought: Vec<&str> = purchases(&events).iter().map(|p| p.class.as_str()).collect();
        let sold: Vec<&str> = sales(&events).iter().map(|s| s.class.as_str()).collect();
        assert_eq!((bought, sold), (vec!["bought"], vec!["sold"]));
        // A purchase answer never books a sale, a sale answer never books a purchase.
        let crossed = [at(1, &sell_line("20.000000", "sold", 1)), at(2, ANSWER_ONE)].concat();
        assert!(parse_text(&crossed, Some(WHO)).is_empty());
    }

    #[test]
    fn a_sale_without_an_answer_or_with_a_failed_one_is_not_booked() {
        let failed = ANSWER_SOLD.replace("result[Success]", "result[Failure]");
        let text = [
            at(1, &sell_line("10.000000", "failed", 1)),
            at(2, &failed),
            at(3, &sell_line("20.000000", "went", 1)),
            at(4, ANSWER_SOLD),
            at(5, &sell_line("30.000000", "unanswered", 1)),
        ]
        .concat();
        let sold: Vec<String> = sales(&parse_text(&text, Some(WHO))).iter().map(|s| s.class.clone()).collect();
        assert_eq!(sold, ["went"]);
        // A sale of no piece is no request.
        let none = [at(1, &sell_line("10.000000", "zero", 0)), at(2, ANSWER_SOLD)].concat();
        assert!(parse_text(&none, Some(WHO)).is_empty());
    }

    #[test]
    fn a_sale_of_goods_at_a_terminal_is_not_read() {
        let text = [at(1, COMMODITY_SOLD), at(2, ANSWER_SOLD), at(3, &sell_line("5.000000", "kiosk", 1)), at(4, ANSWER_SOLD)]
            .concat();
        let events = parse_text(&text, Some(WHO));
        let sold: Vec<&str> = sales(&events).iter().map(|s| s.class.as_str()).collect();
        assert_eq!(sold, ["kiosk"], "the terminal line waits for no answer and takes none");
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn a_sale_keeps_nothing_that_names_the_player_and_reads_back_the_same() {
        let text = [at(1, &sell_line("62.500000", "sold", 3)), at(2, ANSWER_SOLD)].concat();
        let events = parse_text(&text, Some(WHO));
        let entry = session_entry(&text, &events).expect("entry");
        let written = jsonio::dumps(&Value::Object(entry.clone()), None, false);
        for secret in [WHO, "4711", "shopId", "kioskId", "playerId", "[99]", "[77]"] {
            assert!(!written.contains(secret), "{secret} is in {written}");
        }
        assert!(written.contains("\"verkaeufe\""));
        let start = seconds("2026-10-01T10:00:01").expect("stamp");
        let data: Map = vec![("sitzungen".into(), Value::Object(vec![(start.to_string(), Value::Object(entry))]))];
        let read = sessions(&data);
        assert!(read[0].purchases.is_empty());
        assert_eq!(read[0].sales.iter().collect::<Vec<_>>(), sales(&events));
    }

    #[test]
    fn a_drop_stands_in_queued_as_an_interaction() {
        let text = [
            at(1, "[Notice] <Add Inventory Management Move> New Request[7] Player[Spieler] Type[Drop] SourceInventory[4711:Container:0] TargetInventory[INVALID] ItemClass[nvtc_optics_rdot_x1_s1] StoredEntity[NULL]"),
            at(2, &queued(7, "Interaction", "4711:Container:0", "INVALID", "nvtc_optics_rdot_x1_s1", 1, "NONE")),
            at(3, &done(7, "Interaction", "Succeed")),
        ]
        .concat();
        let events = parse_text(&text, Some(WHO));
        let found = movements(&events);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].kind, Kind::Interaction);
        assert_eq!(found[0].class, "nvtc_optics_rdot_x1_s1");
        assert_eq!(found[0].ok, Some(true));
        // The New line alone makes no move: it is no `Queued` line.
        let only_new = at(
            1,
            "[Notice] <Add Inventory Management Move> New Request[7] Player[Spieler] Type[Drop] SourceInventory[INVALID] TargetInventory[INVALID] ItemClass[x]",
        );
        assert!(parse_text(&only_new, Some(WHO)).is_empty());
    }

    #[test]
    fn the_direction_comes_from_queued_not_from_the_swapped_new_line() {
        let text = [
            at(1, "[Notice] <Add Inventory Management Move> New Request[27] Player[Spieler] Type[Store] SourceInventory[4711:Container:0] TargetInventory[INVALID] ItemClass[Fuse_subItem_standard] StoredEntity[Fuse_subItem_standard_468366566217]"),
            at(2, &queued(27, "Store", "INVALID", "4711:Location:3170699229", "NULL", 0, "Fuse_subItem_standard_468366566217")),
        ]
        .concat();
        let events = parse_text(&text, Some(WHO));
        let found = movements(&events);
        assert_eq!(found[0].from, Endpoint::Nowhere);
        assert_eq!(found[0].to, Endpoint::Location("3170699229".into()));
        assert_eq!(found[0].to.place(), Some("3170699229"));
        assert_eq!(found[0].class, "Fuse_subItem_standard", "the serial of the entity falls away");
    }

    #[test]
    fn the_same_number_with_two_kinds_gives_two_moves_with_their_own_results() {
        let text = [
            at(1, &queued(4, "Store", "INVALID", "4711:Container:0", "NULL", 0, "a_class_100000")),
            at(2, &queued(4, "QueryInventory", "INVALID", "INVALID", "NULL", 0, "NONE")),
            at(3, &done(4, "QueryInventory", "Failed")),
            at(4, &done(4, "Store", "Succeed")),
            at(5, &queued(4, "Move", "4711:Container:0", "4711:Location:9", "second_class", 1, "NONE")),
        ]
        .concat();
        let events = parse_text(&text, Some(WHO));
        let found = movements(&events);
        assert_eq!(found.len(), 2, "the query is no move");
        assert_eq!((found[0].kind, found[0].ok), (Kind::Store, Some(true)));
        assert_eq!((found[1].kind, found[1].ok), (Kind::Move, None));
    }

    #[test]
    fn two_open_moves_with_one_number_get_their_own_results() {
        let text = [
            at(1, &queued(4, "Store", "INVALID", "4711:Container:0", "NULL", 0, "a_class_100000")),
            at(2, &queued(4, "Move", "4711:Container:0", "4711:Location:9", "second_class", 1, "NONE")),
            at(3, &done(4, "Move", "Failed")),
            at(4, &done(4, "Store", "Succeed")),
        ]
        .concat();
        let events = parse_text(&text, Some(WHO));
        let found = movements(&events);
        assert_eq!(found.len(), 2);
        assert_eq!((found[0].kind, found[0].ok), (Kind::Store, Some(true)));
        assert_eq!((found[1].kind, found[1].ok), (Kind::Move, Some(false)));
    }

    #[test]
    fn a_log_of_the_early_form_has_no_result_and_counts_as_unconfirmed() {
        let text = at(
            1,
            "[Notice] <InventoryManagementRequest> Queued Request[1] Type[Store] for 'Spieler' [4711] Source Inventory[INVALID] Target Inventory[4711:Container:0]. Source[NULL] amount[0] rank[]. Target[NULL] amount[0] rank[]. Item[x_class_2000000] action[None]. RequestInProgress[0] CurrentProcess[] [Team]",
        );
        let events = parse_text(&text, Some(WHO));
        let found = movements(&events);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].ok, None, "no result line: not a success");
        assert_eq!(found[0].class, "x_class");
    }

    #[test]
    fn a_line_with_a_second_level_mark_reads_like_any_other() {
        let line = queued(5, "Move", "4711:Location:9", "4711:Container:0", "arma_barrel_flhd_s2", 1, "NONE");
        let text = at(1, &line.replace("[Notice]", "[SPAM 36][Notice]"));
        let events = parse_text(&text, Some(WHO));
        assert_eq!(movements(&events).len(), 1);
        assert_eq!(movements(&events)[0].from, Endpoint::Location("9".into()));
        assert_eq!(movements(&events)[0].to, Endpoint::Container);
    }

    #[test]
    fn queries_and_repooling_make_nothing() {
        let text = [
            at(1, &queued(1, "QueryInventory", "INVALID", "INVALID", "NULL", 0, "NONE")),
            at(2, &queued(2, "QueryOrCreate", "INVALID", "INVALID", "NULL", 0, "NONE")),
            at(3, &queued(3, "AmmoRepool", "INVALID", "INVALID", "NULL", 0, "NONE")),
            at(4, &queued(4, "OpenNestedInventory", "INVALID", "INVALID", "NULL", 0, "NONE")),
            at(5, &done(3, "AmmoRepool", "Succeed")),
        ]
        .concat();
        assert!(parse_text(&text, Some(WHO)).is_empty());
    }

    #[test]
    fn every_kind_is_read_with_its_ends_and_amount() {
        let text = [
            at(1, &queued(1, "Split", "4711:Location:9", "4711:Location:9", "behr_lmg_ballistic_01_mag", 20, "NONE")),
            at(2, &queued(2, "Stack", "4711:Container:0", "4711:Container:0", "ksar_gren_frag_01", 1, "NONE")),
            at(3, &queued(3, "StackAll", "4711:Location:9", "4711:Location:9", "NULL", 0, "NONE")),
            at(4, &queued(4, "Interaction", "4711:Container:0", "INVALID", "Fuse_subItem_standard", 1, "NONE")),
            at(5, &queued(5, "Move", "4711:Container:0", "elsewhere", "crlf_medgun_01", 3, "NONE")),
        ]
        .concat();
        let events = parse_text(&text, Some(WHO));
        let kinds: Vec<Kind> = movements(&events).iter().map(|m| m.kind).collect();
        assert_eq!(kinds, [Kind::Split, Kind::Stack, Kind::StackAll, Kind::Interaction, Kind::Move]);
        assert_eq!(movements(&events)[0].amount, 20);
        assert_eq!(movements(&events)[2].class, "");
        assert_eq!(movements(&events)[4].to, Endpoint::Other);
    }

    #[test]
    fn a_failed_result_is_no_success() {
        let text = [
            at(1, &queued(8, "Store", "INVALID", "4711:Container:0", "NULL", 0, "x_class")),
            at(2, &done(8, "Store", "Failed")),
            at(3, &queued(9, "Store", "INVALID", "4711:Container:0", "NULL", 0, "x_class")),
            at(4, &done(9, "Store", "Succeed")),
        ]
        .concat();
        let events = parse_text(&text, Some(WHO));
        let oks: Vec<Option<bool>> = movements(&events).iter().map(|m| m.ok).collect();
        assert_eq!(oks, [Some(false), Some(true)]);
    }

    #[test]
    fn the_price_is_for_all_pieces_and_the_total_stays() {
        let text = [at(1, &buy_way_one("1560.000000", "Drink_bottle_cruz_01_lux_a", 3)), at(2, ANSWER_ONE)].concat();
        let events = parse_text(&text, Some(WHO));
        let found = purchases(&events);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].quantity, 3);
        assert!((found[0].price_total - 1560.0).abs() < f64::EPSILON, "the total stays");
        assert!((found[0].unit_price() - 520.0).abs() < f64::EPSILON);
        assert_eq!(found[0].class, "Drink_bottle_cruz_01_lux_a");
        assert_eq!(found[0].shop, "SCShop_Kiosk");
        assert_eq!(found[0].time, seconds("2026-10-01T10:00:01").expect("stamp"));
        // A price that does not divide stays a fraction.
        let odd = [at(1, &buy_way_one("100.000000", "x", 3)), at(2, ANSWER_ONE)].concat();
        let events = parse_text(&odd, Some(WHO));
        assert!((purchases(&events)[0].unit_price() - 100.0 / 3.0).abs() < 1e-9);
    }

    #[test]
    fn two_purchases_take_two_answers_and_the_second_way_answers_without_a_type() {
        let way_two = "[Notice] <CEntityComponentShoppingProvider::SendStandardItemBuyRequest> Sending SShopBuyRequest - \
             playerId[4711] shopId[99] shopName[SCShop_Item] kioskId[77] client_price[450.000000] \
             itemClassGUID[03b3db91-0063-4da2-99f7-16b20ede4209] itemName[grin_multitool_01_tractorbeam] quantity[1] currencyType[UEC] [Team][Shops]";
        let answer_two = "[Notice] <CEntityComponentShoppingProvider::RmShopFlowResponse> Shop Flow Response - playerId[4711] result[Success] [Team][Shops]";
        let text = [
            at(1, &buy_way_one("10.000000", "first", 1)),
            at(2, &buy_way_one("20.000000", "second", 2)),
            at(3, ANSWER_ONE),
            at(4, way_two),
            at(5, ANSWER_ONE),
            at(6, answer_two),
        ]
        .concat();
        let events = parse_text(&text, Some(WHO));
        let found = purchases(&events);
        let classes: Vec<&str> = found.iter().map(|p| p.class.as_str()).collect();
        assert_eq!(classes, ["first", "second", "grin_multitool_01_tractorbeam"]);
        assert!((found[2].price_total - 450.0).abs() < f64::EPSILON);
    }

    #[test]
    fn a_purchase_without_an_answer_or_with_a_failed_one_is_not_booked() {
        let failed = ANSWER_ONE.replace("result[Success]", "result[Failure]");
        let text = [
            at(1, &buy_way_one("10.000000", "lost", 1)),
            at(2, &buy_way_one("20.000000", "failed", 1)),
            at(3, &failed),
            at(4, ANSWER_ONE),
            at(5, &buy_way_one("30.000000", "unanswered", 1)),
        ]
        .concat();
        let events = parse_text(&text, Some(WHO));
        // The first answer belongs to the first request (in order): it failed, the second one went through.
        let classes: Vec<&str> = purchases(&events).iter().map(|p| p.class.as_str()).collect();
        assert_eq!(classes, ["failed"]);
        // An answer to something that is no purchase takes no request with it.
        let selling = ANSWER_ONE.replace("type[Buying]", "type[Selling]");
        let text = [at(1, &buy_way_one("10.000000", "kept", 1)), at(2, &selling), at(3, ANSWER_ONE)].concat();
        let events = parse_text(&text, Some(WHO));
        assert_eq!(purchases(&events).len(), 1);
        // An answer without a request books nothing.
        assert!(parse_text(&at(1, ANSWER_ONE), Some(WHO)).is_empty());
    }

    #[test]
    fn a_purchase_takes_the_place_of_the_last_landing() {
        let change = |from: &str, to: &str| {
            format!(
                "[Notice] <Update Inventory Location> Player [Spieler] is changing location. Landing [{from}] -> [{to}]. Location [0] -> [1]. Pending [0]"
            )
        };
        let text = [
            at(1, &change("0", "815")),
            at(2, &change("815", "0")),
            at(3, &change("0", "4711")),
            at(4, &buy_way_one("10.000000", "here", 1)),
            at(5, ANSWER_ONE),
            at(6, &change("4711", "0")),
            at(7, &buy_way_one("10.000000", "flying", 1)),
            at(8, ANSWER_ONE),
        ]
        .concat();
        let events = parse_text(&text, Some(WHO));
        let places: Vec<Option<&str>> = purchases(&events).iter().map(|p| p.place.as_deref()).collect();
        assert_eq!(places, [Some("4711"), None]);
    }

    #[test]
    fn a_foreign_account_counts_nothing() {
        let text = [
            at(1, &queued(1, "Store", "INVALID", "4711:Container:0", "NULL", 0, "x_class")),
            at(2, &done(1, "Store", "Succeed")),
            at(3, &buy_way_one("10.000000", "bought", 1)),
            at(4, ANSWER_ONE),
        ]
        .concat();
        assert_eq!(parse_text(&text, Some("spieler")).len(), 2, "the handle is compared in lower case");
        // Another account: its moves do not count, and the purchase is known by number to belong to Spieler.
        assert!(parse_text(&text, Some("Zweit")).is_empty());
        // Without an account everyone counts.
        assert_eq!(parse_text(&text, None).len(), 2);
        // A purchase whose number no line ties to a handle counts for the log's player.
        let alone = [at(1, &buy_way_one("10.000000", "bought", 1)), at(2, ANSWER_ONE)].concat();
        assert_eq!(parse_text(&alone, Some("Zweit")).len(), 1);
    }

    #[test]
    fn nothing_that_names_the_player_is_kept() {
        let text = [
            at(1, &queued(1, "Store", "INVALID", "4711:Container:0", "NULL", 0, "x_class_1234567")),
            at(2, &done(1, "Store", "Succeed")),
            at(3, &buy_way_one("10.000000", "bought", 1)),
            at(4, ANSWER_ONE),
        ]
        .concat();
        let events = parse_text(&text, Some(WHO));
        assert_eq!(events.len(), 2);
        let shown = format!("{events:?}");
        for secret in [WHO, "4711", "shopId", "kioskId", "playerId", "[99]", "[77]"] {
            assert!(!shown.contains(secret), "{secret} is in {shown}");
        }
        // Nor does the entry of the journal, as it is written to the file.
        let entry = session_entry(&text, &events).expect("entry");
        let written = jsonio::dumps(&Value::Object(entry), None, false);
        for secret in [WHO, "4711", "shopId", "kioskId", "playerId", "[99]", "[77]"] {
            assert!(!written.contains(secret), "{secret} is in {written}");
        }
    }

    #[test]
    fn the_entries_of_the_journal_read_back_the_same() {
        let text = [
            at(1, &queued(1, "Store", "INVALID", "4711:Location:900", "NULL", 0, "x_class_1234567")),
            at(2, &done(1, "Store", "Succeed")),
            at(3, &queued(2, "Move", "4711:Location:900", "4711:Container:0", "y_class", 2, "NONE")),
            at(4, &queued(3, "StackAll", "elsewhere", "INVALID", "NULL", 0, "NONE")),
            at(5, &buy_way_one("1560.500000", "bought", 3)),
            at(6, ANSWER_ONE),
        ]
        .concat();
        let events = parse_text(&text, None);
        let entry = session_entry(&text, &events).expect("entry");
        let (start, end) =
            (seconds("2026-10-01T10:00:01").expect("stamp"), seconds("2026-10-01T10:00:06").expect("stamp"));
        assert_eq!(entry[0], ("von".to_string(), Value::int(start)));
        assert_eq!(entry[1], ("bis".to_string(), Value::int(end)));
        let data: Map = vec![("sitzungen".into(), Value::Object(vec![(start.to_string(), Value::Object(entry))]))];
        let sessions = sessions(&data);
        assert_eq!(sessions.len(), 1);
        let back: Vec<Event> = sessions[0]
            .movements
            .iter()
            .cloned()
            .map(Event::Movement)
            .chain(sessions[0].purchases.iter().cloned().map(Event::Purchase))
            .collect();
        let mut expected = events.clone();
        expected.sort_by_key(|e| matches!(e, Event::Purchase(_)));
        assert_eq!(back, expected, "moves with and without a result, a purchase with a fractional price");
        // No event, no entry; stamps that make no session, no entry.
        assert!(session_entry(&text, &[]).is_none());
        assert!(session_entry("no stamp at all", &events).is_none());
    }

    mod file {
        use super::*;
        use crate::legacy;
        use crate::paths::Host;

        fn login(handle: &str) -> String {
            at(0, &format!("[Notice] <Legacy login response> [CIG-net] User Login Success - Handle[{handle}] - Time"))
        }

        fn store_line(n: u32, second: u32) -> String {
            at(second, &queued(n, "Store", "INVALID", "4711:Container:0", "NULL", 0, "x_class_1234567"))
        }

        struct Setup {
            root: std::path::PathBuf,
            paths: Paths,
        }

        impl Setup {
            fn new(tag: &str) -> Setup {
                let root =
                    std::env::temp_dir().join(format!("versekit-lagerjournal-{tag}-{}", crate::testdir::run_tag()));
                let _ = fs::remove_dir_all(&root);
                fs::create_dir_all(root.join("home")).expect("folder");
                fs::create_dir_all(root.join("logs")).expect("folder");
                let host = Host::new(
                    Flavor::native(),
                    &[(legacy::ENV_HOME, root.join("home").to_str().expect("path"))],
                    root.to_str().expect("path"),
                );
                Setup { paths: Paths::new(host), root }
            }

            fn log(&self, name: &str, text: &str) -> String {
                let path = self.root.join("logs").join(name);
                fs::write(&path, text).expect("file");
                path.to_str().expect("path").to_string()
            }

            fn journal(&self) -> String {
                fs::read_to_string(self.paths.app_file(FILE)).unwrap_or_default()
            }

            fn sessions(&self) -> Vec<Session> {
                sessions(&load(&self.paths))
            }
        }

        impl Drop for Setup {
            fn drop(&mut self) {
                let _ = fs::remove_dir_all(&self.root);
            }
        }

        #[test]
        fn reading_twice_gives_the_same_file() {
            let setup = Setup::new("zweimal");
            let file = setup.log("a.log", &[login(WHO), store_line(1, 5), store_line(2, 9)].concat());
            let src = LogSource::new(&setup.paths, LogSource::sources_of(&setup.paths));
            assert_eq!(catch_up(&src, std::slice::from_ref(&file)), 1);
            let first = setup.journal();
            assert_eq!(setup.sessions()[0].movements.len(), 2);
            assert_eq!(catch_up(&src, std::slice::from_ref(&file)), 0, "known by name and size");
            assert_eq!(setup.journal(), first, "the file is untouched");
            // Reading it again from scratch gives the same file as well.
            assert_eq!(rescan(&src, std::slice::from_ref(&file)), 0, "no new session");
            assert_eq!(setup.journal(), first);
        }

        #[test]
        fn a_renamed_game_log_counts_once_and_a_grown_one_replaces_its_session() {
            let setup = Setup::new("umbenannt");
            let src = LogSource::new(&setup.paths, LogSource::sources_of(&setup.paths));
            let early = [login(WHO), store_line(1, 5)].concat();
            let running = setup.log("Game.log", &early);
            assert_eq!(catch_up(&src, std::slice::from_ref(&running)), 1);
            // The log grows: its session is replaced, not added.
            let grown = [early.clone(), store_line(2, 9)].concat();
            setup.log("Game.log", &grown);
            assert_eq!(catch_up(&src, std::slice::from_ref(&running)), 0);
            assert_eq!(setup.sessions().len(), 1);
            assert_eq!(setup.sessions()[0].movements.len(), 2);
            // At the next start the game renames it; the same session under another name is not a second one.
            let backup =
                setup.log("Game Build(1) 01 Oct 26 (10 00 00).log", &[grown.clone(), at(30, "[Notice] end")].concat());
            assert_eq!(catch_up(&src, &[backup]), 0);
            assert_eq!(setup.sessions().len(), 1);
            assert_eq!(setup.sessions()[0].movements.len(), 2);
        }

        #[test]
        fn a_log_of_another_account_is_read_but_records_nothing() {
            let setup = Setup::new("konto");
            let own_a =
                setup.log("a.log", &[login(WHO), at(0, "x"), store_line(1, 5)].concat().replace("T10:00:", "T11:00:"));
            let own_b =
                setup.log("b.log", &[login(WHO), store_line(1, 5)].concat().replace("2026-10-01", "2026-10-02"));
            // The moves of the foreign log name the foreign handle, as in a real log of another account.
            let foreign = setup.log(
                "c.log",
                &[login("Zweit"), store_line(1, 5)]
                    .concat()
                    .replace("2026-10-01", "2026-10-03")
                    .replace(&format!("for '{WHO}'"), "for 'Zweit'"),
            );
            let src = LogSource::new(&setup.paths, LogSource::sources_of(&setup.paths));
            let files = [own_a, own_b, foreign];
            assert_eq!(catch_up(&src, &files), 2);
            assert_eq!(setup.sessions().len(), 2);
            assert_eq!(catch_up(&src, &files), 0, "the foreign log is marked as read");
            assert!(!setup.journal().contains("Zweit") && !setup.journal().contains(WHO), "no handle in the file");
        }

        #[test]
        fn nothing_is_read_while_the_setting_is_off_and_a_new_format_reads_again() {
            let setup = Setup::new("schalter");
            let file = setup.log("a.log", &[login(WHO), store_line(1, 5)].concat());
            let src = LogSource::new(&setup.paths, LogSource::sources_of(&setup.paths));
            setup.paths.set_setting(SETTING, Value::Bool(false));
            assert_eq!(catch_up(&src, std::slice::from_ref(&file)), 0);
            assert!(setup.journal().is_empty(), "no file written");
            setup.paths.set_setting(SETTING, Value::Bool(true));
            assert_eq!(catch_up(&src, std::slice::from_ref(&file)), 1);
            // Another format keeps the sessions and drops only the read position.
            let mut data = load(&setup.paths);
            map_set(&mut data, "format", Value::int(FORMAT + 1));
            fs::write(setup.paths.app_file(FILE), jsonio::dumps(&Value::Object(data), None, false)).expect("file");
            let reloaded = load(&setup.paths);
            assert_eq!(get(&reloaded, "gelesen"), Some(&Value::Object(Vec::new())));
            assert_eq!(sessions(&reloaded).len(), 1);
            assert_eq!(catch_up(&src, std::slice::from_ref(&file)), 0, "read again, the session was there");
            assert_eq!(number(get(&load(&setup.paths), "format")), Some(FORMAT as f64));
        }

        #[test]
        fn the_switch_is_on_without_a_setting_and_is_stored_by_the_setter() {
            let setup = Setup::new("setter");
            assert!(enabled(&setup.paths), "on by default");
            assert!(set_enabled(&setup.paths, false));
            assert!(!enabled(&setup.paths));
            assert!(!setup.paths.setting_bool(SETTING, true), "the key `lager_aus_log` holds it");
            assert!(set_enabled(&setup.paths, true));
            assert!(enabled(&setup.paths));
        }

        #[test]
        fn a_rescan_keeps_the_session_of_a_log_that_is_gone() {
            let setup = Setup::new("neu");
            let src = LogSource::new(&setup.paths, LogSource::sources_of(&setup.paths));
            let gone = setup.log("gone.log", &[login(WHO), store_line(1, 5)].concat());
            let stays =
                setup.log("stays.log", &[login(WHO), store_line(1, 5)].concat().replace("2026-10-01", "2026-10-02"));
            assert_eq!(catch_up(&src, &[gone, stays.clone()]), 2);
            assert_eq!(rescan(&src, std::slice::from_ref(&stays)), 0);
            assert_eq!(setup.sessions().len(), 2, "the session of the vanished log stays");
        }
    }
}
