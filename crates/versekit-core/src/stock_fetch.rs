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

//! Fetching the two lists the stock page reads (`stock_catalog.update` and
//! `mining.update` of 4.12), in the file format `stock_data` reads.
//!
//! * The goods list `lager-katalog.json`: every item of the game related
//!   UEX categories of the kind `item` and every UEX commodity, with the UEX
//!   number. It follows the patch (the stamp `spielstand`), not the clock;
//!   `SHELF_LIFE` is only the emergency limit, `UNSTAMPED_LIFE` the limit of
//!   a file without a stamp.
//! * The mining data `mining-data.json` of scmdb, one file per game build.
//!
//! Like 4.12, a failed request leaves the old file as it is and nothing
//! half-written behind. A write the write lock refuses is no error
//! ([`Outcome::Refused`]). All requests go through a function object, so the
//! tests run without a network.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Mutex, MutexGuard};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::jsonio::{self, Newline, Value};
use crate::paths::Paths;
use crate::scmdb::{self, FetchError};
use crate::stock_data::{CATALOG_FILE, MINING_FILE, decimal_digit, format_is, py_float_value, text_of, truthy};
use crate::text::py_strip;
use crate::{errors, writeguard};

/// The list of the UEX categories.
pub const CATEGORIES_URL: &str = "https://api.uexcorp.uk/2.0/categories";
/// The start of the address of the items of one category (the number follows).
pub const ITEMS_URL: &str = "https://api.uexcorp.uk/2.0/items?id_category=";
/// The list of the UEX commodities.
pub const COMMODITIES_URL: &str = "https://api.uexcorp.uk/2.0/commodities";
/// The game versions UEX knows (which one is live).
pub const GAME_VERSIONS_URL: &str = "https://api.uexcorp.uk/2.0/game_versions";

/// The file that remembers which game build is live.
pub const BUILD_FILE: &str = "spielstand.json";

/// The format number of all three files.
const FORMAT: i64 = 1;

const DAY: f64 = 24.0 * 60.0 * 60.0;
/// The emergency limit of the goods list when the live build is unknown.
pub const SHELF_LIFE: f64 = 90.0 * DAY;
/// How long a goods list without a stamp of the game build is used.
pub const UNSTAMPED_LIFE: f64 = 7.0 * DAY;
/// How long the remembered live build is used without asking again.
pub const BUILD_LIFE: f64 = DAY / 2.0;

/// Requests of the item lists made at the same time.
pub const WORKERS: usize = 4;

/// UEX cuts an answer silently at this many rows.
const CAP: usize = 500;

/// A function object that fetches an address (the UEX lists) or a file name
/// (the scmdb file) and returns its bytes. It is called from several threads.
pub type Fetch<'a> = &'a (dyn Fn(&str) -> Result<Vec<u8>, FetchError> + Sync);

/// Only one update at a time (like `_LOCK` of 4.12).
static LOCK: Mutex<()> = Mutex::new(());

fn lock() -> MutexGuard<'static, ()> {
    LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

// ------------------------------------------------------------ results

/// Which request a [`Reason`] speaks of.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    Categories,
    /// The items of the category with this UEX number.
    Items(i64),
    Commodities,
    GameVersions,
    Mining,
}

impl Source {
    /// The name in the error log (`uex.fetch.<label>` of 4.12).
    fn label(self) -> &'static str {
        match self {
            Source::Categories => "stock_catalog.categories",
            Source::Items(_) => "stock_catalog.items",
            Source::Commodities => "stock_catalog.commodities",
            Source::GameVersions => "gamebuild",
            Source::Mining => "mining",
        }
    }
}

/// Why an update failed (the old file stays as it is).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Reason {
    /// The request failed.
    Fetch(Source, FetchError),
    /// The answer is no JSON of the expected shape.
    Shape(Source),
    /// The answer holds nothing usable.
    Empty(Source),
    /// No build was given.
    NoBuild,
    /// The file could not be written.
    Write(String),
}

impl std::fmt::Display for Reason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Reason::Fetch(source, error) => write!(f, "{}: {error}", source.label()),
            Reason::Shape(source) => write!(f, "{}: unexpected answer", source.label()),
            Reason::Empty(source) => write!(f, "{}: empty answer", source.label()),
            Reason::NoBuild => write!(f, "no build"),
            Reason::Write(what) => write!(f, "not written: {what}"),
        }
    }
}

/// What an update did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Fetched and written.
    Fresh,
    /// Nothing to do: the file is up to date.
    Current,
    /// Fetching is switched off (`SC_BP_NO_NET`) or the network is not
    /// reachable by switch; nothing was changed.
    Offline,
    /// The write lock refused the write; fetched, but nothing was kept.
    Refused,
    /// A request or the write failed; the old file is untouched.
    Failed(Reason),
}

fn fail(reason: Reason) -> Outcome {
    if matches!(reason, Reason::Fetch(_, FetchError::Offline)) { Outcome::Offline } else { Outcome::Failed(reason) }
}

// ------------------------------------------------------------ fetching

/// Whether fetching the lists is switched off on `host` (`SC_BP_NO_NET` of the environment the
/// program was given, not of the shell a check runs in). The `update_*_with` functions of this
/// module leave the switch to the function object they are given (`live_fetcher`).
pub(crate) fn switched_off(host: &crate::paths::Host) -> bool {
    crate::net::offline_in(host)
}

/// The function object for real requests: one connection pool, the time limit
/// of `scmdb`, `user_agent` as the name of the program. With `SC_BP_NO_NET`
/// set for the process, nothing is requested.
pub fn live_fetcher(user_agent: &str) -> impl Fn(&str) -> Result<Vec<u8>, FetchError> + Sync + use<> {
    fetcher(user_agent, crate::net::offline())
}

fn fetcher(user_agent: &str, off: bool) -> impl Fn(&str) -> Result<Vec<u8>, FetchError> + Sync + use<> {
    let agent = (!off).then(|| scmdb::agent(user_agent));
    move |url: &str| match &agent {
        Some(agent) => scmdb::fetch_from(agent, url),
        None => Err(FetchError::Offline),
    }
}

/// Fetches an address and returns the list of its `data` field (`uex.fetch`):
/// no answer without `data` is empty, names are freed of HTML characters. A
/// failure is reported to the error log, except for the network switch.
fn fetch_data(source: Source, url: &str, fetch: Fetch<'_>) -> Result<Value, Reason> {
    let result = fetch(url).map_err(|error| Reason::Fetch(source, error)).and_then(|bytes| parse_data(source, &bytes));
    if let Err(reason) = &result
        && !matches!(reason, Reason::Fetch(_, FetchError::Offline))
    {
        errors::record(&format!("uex.fetch.{}", source.label()), reason);
    }
    result
}

fn parse_data(source: Source, bytes: &[u8]) -> Result<Value, Reason> {
    parse_uex(source.label(), bytes).ok_or(Reason::Shape(source))
}

/// The `data` field of a UEX answer (`uex.fetch`) under the name `label` of
/// the error log; `None` where the answer is no JSON object.
pub(crate) fn parse_uex(label: &str, bytes: &[u8]) -> Option<Value> {
    let text = std::str::from_utf8(bytes).ok()?;
    let Value::Object(map) = jsonio::loads(text).ok()? else {
        return None;
    };
    let mut data = map.into_iter().find(|(key, _)| key == "data").map_or(Value::Null, |(_, value)| value);
    if matches!(data, Value::Null) {
        data = Value::Array(Vec::new());
    }
    // The test on the raw text saves the walk over almost every answer.
    if text.contains('&') {
        unescape(&mut data);
    }
    if let Value::Array(rows) = &data
        && rows.len() == CAP
    {
        errors::record(&format!("uex.fetch.{label}"), &"answer of exactly 500 rows, probably cut");
    }
    Some(data)
}

/// The requests of the item lists, `WORKERS` at a time, in the order of
/// `jobs`. After the first failure no new request starts; the entries not
/// started stay `None`.
fn fetch_all(jobs: &[(Source, String)], fetch: Fetch<'_>) -> Vec<Option<Result<Value, Reason>>> {
    let next = AtomicUsize::new(0);
    let stop = AtomicBool::new(false);
    let slots: Mutex<Vec<Option<Result<Value, Reason>>>> = Mutex::new(jobs.iter().map(|_| None).collect());
    let work = || {
        while !stop.load(Ordering::Relaxed) {
            let index = next.fetch_add(1, Ordering::Relaxed);
            let Some((source, url)) = jobs.get(index) else { break };
            let result = fetch_data(*source, url, fetch);
            if result.is_err() {
                stop.store(true, Ordering::Relaxed);
            }
            if let Ok(mut slots) = slots.lock()
                && let Some(slot) = slots.get_mut(index)
            {
                *slot = Some(result);
            }
        }
    };
    std::thread::scope(|scope| {
        for _ in 1..WORKERS.min(jobs.len()) {
            // A thread that cannot start leaves its share to the others.
            let _ = std::thread::Builder::new().spawn_scoped(scope, work);
        }
        work();
    });
    slots.into_inner().unwrap_or_else(|e| e.into_inner())
}

// ------------------------------------------------------------ reading files

pub(crate) fn seconds(now: SystemTime) -> f64 {
    now.duration_since(UNIX_EPOCH).map_or(0.0, |d| d.as_secs_f64())
}

/// A list file: an object of the right format; anything else counts as absent.
fn load_store(path: &str) -> Option<Value> {
    let value = jsonio::read_json(path).ok()?;
    (matches!(value, Value::Object(_)) && format_is(&value, FORMAT)).then_some(value)
}

/// How old the file is in seconds, or `None` without a (usable) `geholt`.
fn age(store: &Value, now: f64) -> Option<f64> {
    let fetched = store.get("geholt").filter(|value| truthy(Some(value)))?;
    py_float_value(fetched).map(|then| now - then)
}

fn stamp(store: &Value) -> &str {
    text_of(store.get("spielstand"))
}

/// `due` of 4.12 for a goods file already read.
fn due_from(store: Option<&Value>, now: f64, live: &str) -> bool {
    let Some(store) = store else { return true };
    let age = age(store, now);
    if age.is_none_or(|a| a >= SHELF_LIFE) {
        return true;
    }
    let before = stamp(store);
    // A patch since the file was fetched; when either build is unknown, the
    // file stands.
    if !live.is_empty() && !before.is_empty() && before != live {
        return true;
    }
    before.is_empty() && age.is_none_or(|a| a >= UNSTAMPED_LIFE)
}

/// Must the goods list be fetched? Reads only the file. `live` is the live
/// game build ([`live_build`]), `""` when unknown (then no patch is noticed).
pub fn goods_due(paths: &Paths, now: SystemTime, live: &str) -> bool {
    due_from(load_store(&paths.app_file(CATALOG_FILE)).as_ref(), seconds(now), live)
}

/// The live game build remembered by the last [`update_build`], `""` when
/// unknown (`gamebuild.live`).
pub fn live_build(paths: &Paths) -> String {
    load_store(&paths.app_file(BUILD_FILE)).map_or_else(String::new, |store| text_of(store.get("live")).to_string())
}

// ------------------------------------------------------------ writing files

/// Compact JSON: `separators=(',', ':')` and `ensure_ascii=False`.
pub(crate) fn compact(value: &Value) -> String {
    let mut out = String::new();
    encode_compact(&mut out, value);
    out
}

fn encode_compact(out: &mut String, value: &Value) {
    match value {
        Value::Array(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                encode_compact(out, item);
            }
            out.push(']');
        }
        Value::Object(map) => {
            out.push('{');
            for (i, (key, item)) in map.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                jsonio::push_json_str(out, key);
                out.push(':');
                encode_compact(out, item);
            }
            out.push('}');
        }
        // Scalars read the same in every spelling.
        scalar => out.push_str(&jsonio::dumps(scalar, None, false)),
    }
}

/// Write atomically; a write the lock refuses is no error.
fn write(path: &str, text: &str, label: &str) -> Outcome {
    match jsonio::write_text_atomic(path, text, Newline::Keep) {
        Ok(()) => Outcome::Fresh,
        Err(error) if writeguard::refused(&error) => Outcome::Refused,
        Err(error) => {
            errors::record(label, &error);
            Outcome::Failed(Reason::Write(error.to_string()))
        }
    }
}

// ------------------------------------------------------------ game build

/// Asks UEX for the live game build, unless the remembered one is younger than
/// [`BUILD_LIFE`] (`gamebuild.update`).
pub fn update_build(paths: &Paths, user_agent: &str) -> Outcome {
    update_build_with(paths, SystemTime::now(), &live_fetcher(user_agent))
}

/// [`update_build`] with its own function object and time.
pub fn update_build_with(paths: &Paths, now: SystemTime, fetch: Fetch<'_>) -> Outcome {
    let path = paths.app_file(BUILD_FILE);
    let secs = seconds(now);
    if load_store(&path).and_then(|store| age(&store, secs)).is_some_and(|a| a < BUILD_LIFE) {
        return Outcome::Current;
    }
    let data = match fetch_data(Source::GameVersions, GAME_VERSIONS_URL, fetch) {
        Ok(data) => data,
        Err(reason) => return fail(reason),
    };
    if !matches!(data, Value::Object(_)) {
        return Outcome::Failed(Reason::Shape(Source::GameVersions));
    }
    let live = py_strip(text_of(data.get("live")));
    if live.is_empty() {
        return Outcome::Failed(Reason::Empty(Source::GameVersions));
    }
    let store = Value::Object(vec![
        ("live".into(), Value::str(live)),
        ("ptu".into(), Value::str(py_strip(text_of(data.get("ptu"))))),
        ("format".into(), Value::int(FORMAT)),
        ("geholt".into(), Value::Float(secs)),
    ]);
    write(&path, &jsonio::dumps(&store, None, false), "stock_fetch.build")
}

// ------------------------------------------------------------ goods list

/// Fetches the goods list when it is due. For the live game build it first
/// asks UEX (at most every twelve hours); the build becomes the stamp of the
/// file.
pub fn update_goods(paths: &Paths, user_agent: &str) -> Outcome {
    update_goods_with(paths, SystemTime::now(), &live_fetcher(user_agent))
}

/// [`update_goods`] with its own function object and time.
pub fn update_goods_with(paths: &Paths, now: SystemTime, fetch: Fetch<'_>) -> Outcome {
    let _lock = lock();
    // A failed question about the build changes nothing: the remembered one
    // (or none) counts then.
    let _ = update_build_with(paths, now, fetch);
    let live = live_build(paths);
    if !goods_due(paths, now, &live) {
        return Outcome::Current;
    }
    let (parts, goods) = match fetch_lists(fetch) {
        Ok(lists) => lists,
        Err(outcome) => return outcome,
    };
    let mut fields = vec![
        ("teile".to_string(), Value::Array(parts)),
        ("waren".to_string(), Value::Array(goods)),
        ("format".to_string(), Value::int(FORMAT)),
        ("geholt".to_string(), Value::Float(seconds(now))),
    ];
    // The stamp says under which game build the list was fetched.
    let live = live_build(paths);
    if !live.is_empty() {
        fields.push(("spielstand".to_string(), Value::Str(live)));
    }
    write(&paths.app_file(CATALOG_FILE), &compact(&Value::Object(fields)), "stock_fetch.goods")
}

// ------------------------------------------------------------ in the background

/// What one pass of [`keep_with`] did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Kept {
    /// The question for the live game build.
    pub build: Outcome,
    /// The goods list, when it was due.
    pub goods: Option<Outcome>,
}

impl Kept {
    /// Whether the write lock refused a write: on a copy of the data asking again brings nothing.
    pub fn refused(&self) -> bool {
        self.build == Outcome::Refused || self.goods == Some(Outcome::Refused)
    }
}

/// One pass in the background, for the process that runs the whole session (the round of the watcher of 4.12 asks
/// `gamebuild.update`, `stock_catalog.start` fetches the goods list when it is due): the live game build first, so
/// that every list fetched after it carries the stamp of the running game, then the goods list when it is due for
/// that build ([`goods_due`]).
pub fn keep_with(paths: &Paths, now: SystemTime, fetch: Fetch<'_>) -> Kept {
    let build = update_build_with(paths, now, fetch);
    let goods = goods_due(paths, now, &live_build(paths)).then(|| update_goods_with(paths, now, fetch));
    Kept { build, goods }
}

/// One row of a list as `(name, number)`: a name and a positive number.
fn named(row: &Value) -> Option<(String, i64)> {
    let name = py_strip(text_of(row.get("name")));
    let number = row.get("id").and_then(py_int).filter(|number| *number > 0)?;
    (!name.is_empty()).then(|| (name.to_string(), number))
}

fn rows(data: &Value) -> &[Value] {
    match data {
        Value::Array(rows) => rows,
        _ => &[],
    }
}

/// Python's `int(value)`, `None` where it raises.
pub(crate) fn py_int(value: &Value) -> Option<i64> {
    match value {
        Value::Bool(b) => Some(i64::from(*b)),
        Value::Int(n) => n.parse().ok(),
        Value::Float(x) if x.is_finite() => Some(x.trunc() as i64),
        Value::Str(text) => py_int_text(text),
        _ => None,
    }
}

fn py_int_text(text: &str) -> Option<i64> {
    let text = py_strip(text);
    let (negative, body) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text.strip_prefix('+').unwrap_or(text)),
    };
    let chars: Vec<char> = body.chars().collect();
    let mut value: i64 = 0;
    let mut after_digit = false;
    for (i, c) in chars.iter().enumerate() {
        if *c == '_' {
            // Only between two digits.
            if !after_digit || i + 1 == chars.len() {
                return None;
            }
            after_digit = false;
            continue;
        }
        value = value.checked_mul(10)?.checked_add(i64::from(decimal_digit(*c)?))?;
        after_digit = true;
    }
    (!chars.is_empty()).then_some(if negative { -value } else { value })
}

/// The three requests of `update` in 4.12: categories, the items of each game
/// related item category, the commodities. `Err` carries the outcome to give.
fn fetch_lists(fetch: Fetch<'_>) -> Result<(Vec<Value>, Vec<Value>), Outcome> {
    let categories = fetch_data(Source::Categories, CATEGORIES_URL, fetch).map_err(fail)?;
    if rows(&categories).is_empty() {
        return Err(if matches!(categories, Value::Array(_)) {
            Outcome::Failed(Reason::Empty(Source::Categories))
        } else {
            Outcome::Failed(Reason::Shape(Source::Categories))
        });
    }
    let mut chosen: Vec<(i64, String)> = Vec::new();
    for category in rows(&categories) {
        let game_related = matches!(category.get("is_game_related"), Some(Value::Int(n)) if n == "1")
            || category.get("is_game_related").and_then(Value::as_str) == Some("1");
        let is_item = text_of(category.get("type")).to_lowercase() == "item";
        let id = category.get("id").filter(|id| truthy(Some(id)));
        if !(matches!(category, Value::Object(_)) && game_related && is_item) {
            continue;
        }
        let Some(id) = id else { continue };
        // A number that cannot be read would stop 4.12 with an exception.
        let number = py_int(id).ok_or(Outcome::Failed(Reason::Shape(Source::Categories)))?;
        chosen.push((number, py_strip(text_of(category.get("section"))).to_string()));
    }
    let jobs: Vec<(Source, String)> =
        chosen.iter().map(|(id, _)| (Source::Items(*id), format!("{ITEMS_URL}{id}"))).collect();
    let fetched = fetch_all(&jobs, fetch);
    let mut parts = Vec::new();
    for ((_, section), result) in chosen.iter().zip(fetched) {
        // A missing category would let its parts drop out of the list unseen:
        // better to keep the old one.
        let data = match result {
            Some(Ok(data)) => data,
            Some(Err(reason)) => return Err(fail(reason)),
            None => return Err(Outcome::Failed(Reason::Empty(Source::Categories))),
        };
        for row in rows(&data) {
            if let Some((name, number)) = named(row) {
                parts.push(Value::Array(vec![Value::Str(name), Value::int(number), Value::str(section.as_str())]));
            }
        }
    }
    let commodities = fetch_data(Source::Commodities, COMMODITIES_URL, fetch).map_err(fail)?;
    if !truthy(Some(&commodities)) {
        return Err(Outcome::Failed(Reason::Empty(Source::Commodities)));
    }
    let goods: Vec<Value> = rows(&commodities)
        .iter()
        .filter_map(named)
        .map(|(name, number)| Value::Array(vec![Value::Str(name), Value::int(number)]))
        .collect();
    if parts.is_empty() && goods.is_empty() {
        return Err(Outcome::Failed(Reason::Empty(Source::Commodities)));
    }
    Ok((parts, goods))
}

// ------------------------------------------------------------ raw material prices

/// The file of the raw material prices (`prices.py`).
pub const PRICES_FILE: &str = "preise.json";

/// How long the prices count as fresh: a day.
pub const PRICES_LIFE: f64 = DAY;

/// Must the prices be fetched? Without a file, in another format or a day old: yes.
pub fn prices_due(paths: &Paths, now: SystemTime) -> bool {
    let Some(store) = load_store(&paths.app_file(PRICES_FILE)) else { return true };
    age(&store, seconds(now)).is_none_or(|a| a >= PRICES_LIFE)
}

/// Fetches the UEX commodities and keeps, for each material, both forms it
/// has (refined and ore) with the price a terminal asks and the one it pays
/// (`prices.update`). Leaves the old file as it is when anything fails.
pub fn update_prices(paths: &Paths, user_agent: &str) -> Outcome {
    update_prices_with(paths, SystemTime::now(), &live_fetcher(user_agent))
}

/// [`update_prices`] with its own function object and time.
pub fn update_prices_with(paths: &Paths, now: SystemTime, fetch: Fetch<'_>) -> Outcome {
    let _lock = lock();
    if !prices_due(paths, now) {
        return Outcome::Current;
    }
    let data = match fetch_data(Source::Commodities, COMMODITIES_URL, fetch) {
        Ok(data) => data,
        Err(reason) => return fail(reason),
    };
    if rows(&data).is_empty() {
        return Outcome::Failed(Reason::Empty(Source::Commodities));
    }
    // Every material stands twice at UEX, refined and as ore: both forms are
    // kept under the same comparable name, and the caller chooses.
    let mut waren: Vec<(String, Value)> = Vec::new();
    for row in rows(&data) {
        let name = py_strip(text_of(row.get("name")));
        if name.is_empty() {
            continue;
        }
        let number = |key: &str| row.get(key).and_then(py_float_value).unwrap_or(0.0);
        let form = Value::Object(vec![
            ("name".into(), Value::str(name)),
            ("kauf".into(), Value::Float(number("price_buy"))),
            ("verkauf".into(), Value::Float(number("price_sell"))),
        ]);
        let key = crate::stock_data::norm_material(name);
        match waren.iter_mut().find(|(k, _)| *k == key) {
            Some((_, Value::Array(forms))) => forms.push(form),
            _ => waren.push((key, Value::Array(vec![form]))),
        }
    }
    let mut fields = vec![
        ("waren".to_string(), Value::Object(waren)),
        ("format".to_string(), Value::int(FORMAT)),
        ("geholt".to_string(), Value::Float(seconds(now))),
    ];
    let live = live_build(paths);
    if !live.is_empty() {
        fields.push(("spielstand".to_string(), Value::Str(live)));
    }
    write(&paths.app_file(PRICES_FILE), &compact(&Value::Object(fields)), "prices.update")
}

// ------------------------------------------------------------ mining data

/// Is the mining file complete for this build? Then there is nothing to fetch.
///
/// A file of an older program without the refineries or the elements is not.
pub fn mining_current(paths: &Paths, build: &str) -> bool {
    let Some(store) = load_store(&paths.app_file(MINING_FILE)) else { return false };
    let present = |key: &str| !matches!(store.get(key), None | Some(Value::Null));
    store.get("build").and_then(Value::as_str) == Some(build)
        && truthy(store.get("locations"))
        && present("refineries")
        && present("elemente")
}

/// Fetches the mining data of scmdb for `build` (`mining.update`) unless the
/// file is complete for it already.
pub fn update_mining(paths: &Paths, build: &str, user_agent: &str) -> Outcome {
    update_mining_with(paths, build, &|name: &str| scmdb::fetch_file(name, user_agent))
}

/// [`update_mining`] with its own function object, called with the file name
/// (`mining_data-<build>.json`).
pub fn update_mining_with(paths: &Paths, build: &str, fetch_file: Fetch<'_>) -> Outcome {
    let _lock = lock();
    if build.is_empty() {
        return Outcome::Failed(Reason::NoBuild);
    }
    if mining_current(paths, build) {
        return Outcome::Current;
    }
    let bytes = match fetch_file(&format!("mining_data-{build}.json")) {
        Ok(bytes) => bytes,
        Err(error) => {
            let reason = Reason::Fetch(Source::Mining, error);
            if !matches!(reason, Reason::Fetch(_, FetchError::Offline)) {
                errors::record("mining.update", &reason);
            }
            return fail(reason);
        }
    };
    let Some(raw) = std::str::from_utf8(&bytes).ok().and_then(|text| jsonio::loads(text).ok()) else {
        errors::record("mining.update", &Reason::Shape(Source::Mining));
        return Outcome::Failed(Reason::Shape(Source::Mining));
    };
    if !matches!(raw, Value::Object(_)) {
        return Outcome::Failed(Reason::Shape(Source::Mining));
    }
    let or = |key: &str, default: Value| match raw.get(key) {
        Some(value) if truthy(Some(value)) => value.clone(),
        _ => default,
    };
    let locations = or("locations", Value::Array(Vec::new()));
    if !truthy(Some(&locations)) {
        return Outcome::Failed(Reason::Empty(Source::Mining));
    }
    // The refineries belong to it, and the master data of each raw material
    // (rarity and scan signature).
    let store = Value::Object(vec![
        ("format".into(), Value::int(FORMAT)),
        ("build".into(), Value::str(build)),
        ("locations".into(), locations),
        ("compositions".into(), or("compositions", Value::Object(Vec::new()))),
        ("refineries".into(), or("refineries", Value::Array(Vec::new()))),
        ("refineryProfiles".into(), or("refineryProfiles", Value::Object(Vec::new()))),
        ("elemente".into(), or("mineableElements", Value::Object(Vec::new()))),
    ]);
    write(&paths.app_file(MINING_FILE), &jsonio::dumps(&store, None, false), "mining._save")
}

// ------------------------------------------------------------ HTML characters

/// `html.unescape` over all strings of the data (not over the keys).
fn unescape(value: &mut Value) {
    match value {
        Value::Str(text) if text.contains('&') => *text = html_unescape(text),
        Value::Array(items) => items.iter_mut().for_each(unescape),
        Value::Object(map) => map.iter_mut().for_each(|(_, item)| unescape(item)),
        _ => {}
    }
}

/// The names of the characters 160 to 255 (HTML 4, Latin 1). All of them are
/// also valid without the closing semicolon.
const LATIN1: [&str; 96] = [
    "nbsp", "iexcl", "cent", "pound", "curren", "yen", "brvbar", "sect", "uml", "copy", "ordf", "laquo", "not", "shy",
    "reg", "macr", "deg", "plusmn", "sup2", "sup3", "acute", "micro", "para", "middot", "cedil", "sup1", "ordm",
    "raquo", "frac14", "frac12", "frac34", "iquest", "Agrave", "Aacute", "Acirc", "Atilde", "Auml", "Aring", "AElig",
    "Ccedil", "Egrave", "Eacute", "Ecirc", "Euml", "Igrave", "Iacute", "Icirc", "Iuml", "ETH", "Ntilde", "Ograve",
    "Oacute", "Ocirc", "Otilde", "Ouml", "times", "Oslash", "Ugrave", "Uacute", "Ucirc", "Uuml", "Yacute", "THORN",
    "szlig", "agrave", "aacute", "acirc", "atilde", "auml", "aring", "aelig", "ccedil", "egrave", "eacute", "ecirc",
    "euml", "igrave", "iacute", "icirc", "iuml", "eth", "ntilde", "ograve", "oacute", "ocirc", "otilde", "ouml",
    "divide", "oslash", "ugrave", "uacute", "ucirc", "uuml", "yacute", "thorn", "yuml",
];

/// Further names valid without the semicolon.
const LEGACY: [(&str, char); 10] = [
    ("amp", '&'),
    ("AMP", '&'),
    ("lt", '<'),
    ("LT", '<'),
    ("gt", '>'),
    ("GT", '>'),
    ("quot", '"'),
    ("QUOT", '"'),
    ("COPY", '\u{a9}'),
    ("REG", '\u{ae}'),
];

/// Names that need the semicolon. The remaining names of HTML 5 are left as
/// they are.
const WITH_SEMICOLON: [(&str, char); 48] = [
    ("apos", '\''),
    ("euro", '\u{20ac}'),
    ("ndash", '\u{2013}'),
    ("mdash", '\u{2014}'),
    ("lsquo", '\u{2018}'),
    ("rsquo", '\u{2019}'),
    ("sbquo", '\u{201a}'),
    ("ldquo", '\u{201c}'),
    ("rdquo", '\u{201d}'),
    ("bdquo", '\u{201e}'),
    ("hellip", '\u{2026}'),
    ("trade", '\u{2122}'),
    ("bull", '\u{2022}'),
    ("dagger", '\u{2020}'),
    ("Dagger", '\u{2021}'),
    ("permil", '\u{2030}'),
    ("lsaquo", '\u{2039}'),
    ("rsaquo", '\u{203a}'),
    ("OElig", '\u{152}'),
    ("oelig", '\u{153}'),
    ("Scaron", '\u{160}'),
    ("scaron", '\u{161}'),
    ("Yuml", '\u{178}'),
    ("fnof", '\u{192}'),
    ("circ", '\u{2c6}'),
    ("tilde", '\u{2dc}'),
    ("ensp", '\u{2002}'),
    ("emsp", '\u{2003}'),
    ("thinsp", '\u{2009}'),
    ("zwnj", '\u{200c}'),
    ("zwj", '\u{200d}'),
    ("lrm", '\u{200e}'),
    ("rlm", '\u{200f}'),
    ("larr", '\u{2190}'),
    ("uarr", '\u{2191}'),
    ("rarr", '\u{2192}'),
    ("darr", '\u{2193}'),
    ("harr", '\u{2194}'),
    ("minus", '\u{2212}'),
    ("infin", '\u{221e}'),
    ("ne", '\u{2260}'),
    ("le", '\u{2264}'),
    ("ge", '\u{2265}'),
    ("hearts", '\u{2665}'),
    ("spades", '\u{2660}'),
    ("clubs", '\u{2663}'),
    ("diams", '\u{2666}'),
    ("prime", '\u{2032}'),
];

/// A name valid without the semicolon.
fn legacy_entity(name: &str) -> Option<char> {
    if let Some(i) = LATIN1.iter().position(|n| *n == name) {
        return char::from_u32(0xa0 + u32::try_from(i).ok()?);
    }
    LEGACY.iter().find(|(n, _)| *n == name).map(|(_, c)| *c)
}

/// A name that ended in a semicolon.
fn entity(name: &str) -> Option<char> {
    legacy_entity(name).or_else(|| WITH_SEMICOLON.iter().find(|(n, _)| *n == name).map(|(_, c)| *c))
}

/// What a numeric character reference stands for (`html._replace_charref`).
fn char_reference(number: u64) -> String {
    const WINDOWS_1252: [u32; 32] = [
        0x20ac, 0x81, 0x201a, 0x192, 0x201e, 0x2026, 0x2020, 0x2021, 0x2c6, 0x2030, 0x160, 0x2039, 0x152, 0x8d, 0x17d,
        0x8f, 0x90, 0x2018, 0x2019, 0x201c, 0x201d, 0x2022, 0x2013, 0x2014, 0x2dc, 0x2122, 0x161, 0x203a, 0x153, 0x9d,
        0x17e, 0x178,
    ];
    let replacement = |c: char| c.to_string();
    match number {
        0 => replacement('\u{fffd}'),
        0x0d => replacement('\r'),
        0x80..=0x9f => {
            let code = WINDOWS_1252.get(usize::try_from(number - 0x80).unwrap_or(0)).copied().unwrap_or(0xfffd);
            replacement(char::from_u32(code).unwrap_or('\u{fffd}'))
        }
        0xd800..=0xdfff => replacement('\u{fffd}'),
        n if n > 0x10ffff => replacement('\u{fffd}'),
        // Characters that are no text: dropped.
        0x1..=0x8 | 0xb | 0xe..=0x1f | 0x7f | 0xfdd0..=0xfdef => String::new(),
        n if n & 0xfffe == 0xfffe => String::new(),
        n => u32::try_from(n).ok().and_then(char::from_u32).map(replacement).unwrap_or_default(),
    }
}

/// The reference after the `&` at `start`: its replacement and the position
/// behind it, or `None` when it is none.
fn reference_at(chars: &[char], start: usize) -> Option<(String, usize)> {
    if chars.get(start) == Some(&'#') {
        let hex = matches!(chars.get(start + 1), Some('x' | 'X'));
        let (radix, from) = if hex { (16, start + 2) } else { (10, start + 1) };
        let mut end = from;
        let mut number: u64 = 0;
        while let Some(digit) = chars.get(end).and_then(|c| c.to_digit(radix)) {
            number = number.saturating_mul(u64::from(radix)).saturating_add(u64::from(digit));
            end += 1;
        }
        if end == from {
            return None;
        }
        if chars.get(end) == Some(&';') {
            end += 1;
        }
        return Some((char_reference(number), end));
    }
    let mut end = start;
    while end < chars.len()
        && end - start < 32
        && !matches!(chars[end], '\t' | '\n' | '\u{c}' | ' ' | '<' | '&' | '#' | ';')
    {
        end += 1;
    }
    if end == start {
        return None;
    }
    let mut word: Vec<char> = chars[start..end].to_vec();
    if chars.get(end) == Some(&';') {
        word.push(';');
        end += 1;
    }
    let name: String = word.iter().collect();
    if let Some(name) = name.strip_suffix(';')
        && let Some(found) = entity(name)
    {
        return Some((found.to_string(), end));
    }
    if !name.ends_with(';')
        && let Some(found) = legacy_entity(&name)
    {
        return Some((found.to_string(), end));
    }
    // A known name without semicolon in front of more text (`&ampfoo`).
    (2..word.len()).rev().find_map(|length| {
        let prefix: String = word[..length].iter().collect();
        legacy_entity(&prefix).map(|found| {
            let rest: String = word[length..].iter().collect();
            (format!("{found}{rest}"), end)
        })
    })
}

/// Python's `html.unescape`, for the names of the table above and every
/// numeric reference.
pub(crate) fn html_unescape(text: &str) -> String {
    if !text.contains('&') {
        return text.to_string();
    }
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < chars.len() {
        if chars[i] != '&' {
            out.push(chars[i]);
            i += 1;
            continue;
        }
        match reference_at(&chars, i + 1) {
            Some((replacement, end)) => {
                out.push_str(&replacement);
                i = end;
            }
            None => {
                out.push('&');
                i += 1;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::fs;
    use std::sync::atomic::AtomicU32;
    use std::time::Duration;

    use serde_json::json;

    use super::*;
    use crate::paths::Host;
    use crate::pypath::Flavor;
    use crate::stock_data::StockData;

    static COUNTER: AtomicU32 = AtomicU32::new(0);

    /// A data folder of its own, removed afterwards.
    struct Folder {
        dir: String,
        paths: Paths,
    }

    impl Folder {
        fn new() -> Folder {
            let number = COUNTER.fetch_add(1, Ordering::Relaxed);
            let dir = std::env::temp_dir().join(format!("versekit-fetch-{}-{number}", crate::testdir::run_tag()));
            let dir = dir.to_string_lossy().into_owned();
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).unwrap();
            let paths = Paths::new(Host::new(Flavor::native(), &[(crate::legacy::ENV_HOME, dir.as_str())], &dir));
            Folder { dir, paths }
        }

        fn file(&self, name: &str) -> String {
            self.paths.app_file(name)
        }

        fn put(&self, name: &str, text: &str) {
            fs::write(self.file(name), text).unwrap();
        }

        fn text(&self, name: &str) -> Option<String> {
            fs::read_to_string(self.file(name)).ok()
        }

        /// No temporary file is left behind.
        fn leftovers(&self) -> Vec<String> {
            fs::read_dir(&self.dir)
                .unwrap()
                .filter_map(|e| e.ok())
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .filter(|n| n.ends_with(".tmp"))
                .collect()
        }
    }

    impl Drop for Folder {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.dir);
        }
    }

    /// Prepared answers by address, and the addresses asked for.
    struct Fake {
        answers: HashMap<String, Result<Vec<u8>, FetchError>>,
        calls: Mutex<Vec<String>>,
    }

    impl Fake {
        fn new() -> Fake {
            Fake { answers: HashMap::new(), calls: Mutex::new(Vec::new()) }
        }

        fn data(mut self, url: &str, data: serde_json::Value) -> Fake {
            self.answers.insert(url.to_string(), Ok(json!({ "status": "ok", "data": data }).to_string().into_bytes()));
            self
        }

        fn raw(mut self, url: &str, bytes: &[u8]) -> Fake {
            self.answers.insert(url.to_string(), Ok(bytes.to_vec()));
            self
        }

        fn error(mut self, url: &str, error: FetchError) -> Fake {
            self.answers.insert(url.to_string(), Err(error));
            self
        }

        fn fetch(&self, url: &str) -> Result<Vec<u8>, FetchError> {
            self.calls.lock().unwrap().push(url.to_string());
            self.answers.get(url).cloned().unwrap_or(Err(FetchError::Missing))
        }

        fn calls(&self) -> Vec<String> {
            let mut calls = self.calls.lock().unwrap().clone();
            calls.sort();
            calls
        }
    }

    fn at(secs: u64) -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(secs)
    }

    const NOW: u64 = 1_800_000_000;

    fn item_url(id: u32) -> String {
        format!("{ITEMS_URL}{id}")
    }

    /// The answers of a small UEX: three categories (one a commodity group,
    /// one not game related), one game version.
    fn uex() -> Fake {
        Fake::new()
            .data(GAME_VERSIONS_URL, json!({ "live": "4.10.2", "ptu": null }))
            .data(
                CATEGORIES_URL,
                json!([
                    { "id": 1, "type": "item", "section": "Armor", "is_game_related": 1 },
                    { "id": "2", "type": "Item", "section": " Liveries ", "is_game_related": "1" },
                    { "id": 3, "type": "item", "section": "Outside", "is_game_related": 0 },
                    { "id": 4, "type": "service", "section": "Services", "is_game_related": 1 },
                    { "id": 0, "type": "item", "section": "Zero", "is_game_related": 1 },
                    "not an object"
                ]),
            )
            .data(
                &item_url(1),
                json!([
                    { "id": 171, "name": "  Venture Arms  " },
                    { "id": 172, "name": "" },
                    { "id": 0, "name": "No Number" },
                    { "id": "173", "name": "Grey&apos;s Helmet" },
                    { "id": null, "name": "Null Number" },
                    { "name": "No Id" },
                    "row"
                ]),
            )
            .data(&item_url(2), json!([{ "id": 4245, "name": "Archimedes &amp; Merlin Livery" }]))
            .data(
                COMMODITIES_URL,
                json!([
                    { "id": 1, "name": "Agricium" },
                    { "id": 2, "name": " Agricium (Ore) " },
                    { "id": 9, "name": "Venture Arms" },
                    { "id": -1, "name": "Negative" }
                ]),
            )
    }

    fn expected_goods(live: &str, geholt: &str) -> String {
        format!(
            "{{\"teile\":[[\"Venture Arms\",171,\"Armor\"],[\"Grey's Helmet\",173,\"Armor\"],\
             [\"Archimedes & Merlin Livery\",4245,\"Liveries\"]],\
             \"waren\":[[\"Agricium\",1],[\"Agricium (Ore)\",2],[\"Venture Arms\",9]],\
             \"format\":1,\"geholt\":{geholt},\"spielstand\":\"{live}\"}}"
        )
    }

    // ---------------------------------------------------------- html

    #[test]
    fn html_characters_like_python() {
        let cases = [
            ("Grey&apos;s Market", "Grey's Market"),
            ("A &amp; B", "A & B"),
            ("&#39;&#x27;&#X27;", "'''"),
            ("&lt;b&gt; &quot;x&quot;", "<b> \"x\""),
            ("caf&eacute; &Eacute;", "caf\u{e9} \u{c9}"),
            ("&ampfoo", "&foo"),
            ("&amp", "&"),
            ("&unknown; &", "&unknown; &"),
            ("& and &# and &#x;", "& and &# and &#x;"),
            ("&#0;&#128;&#13;&#xD800;&#x110000;&#1;", "\u{fffd}\u{20ac}\r\u{fffd}\u{fffd}"),
            ("&#99999999999999999999;", "\u{fffd}"),
            ("no entity", "no entity"),
        ];
        for (given, wanted) in cases {
            assert_eq!(html_unescape(given), wanted, "{given}");
        }
    }

    #[test]
    fn only_values_are_freed_of_html_characters() {
        let mut value = jsonio::loads(r#"{"a&amp;b": ["x&amp;y", 1, null, {"k": "&lt;"}]}"#).unwrap();
        unescape(&mut value);
        assert_eq!(compact(&value), r#"{"a&amp;b":["x&y",1,null,{"k":"<"}]}"#);
    }

    #[test]
    fn integers_like_python() {
        let value = |text: &str| py_int(&jsonio::loads(text).unwrap());
        assert_eq!(value("12"), Some(12));
        assert_eq!(value("\" 12 \""), Some(12));
        assert_eq!(value("\"1_000\""), Some(1000));
        assert_eq!(value("\"+7\""), Some(7));
        assert_eq!(value("12.9"), Some(12));
        assert_eq!(value("true"), Some(1));
        for bad in ["\"12.5\"", "\"\"", "\"_1\"", "\"1_\"", "\"1__0\"", "\"abc\"", "null", "[]"] {
            assert_eq!(value(bad), None, "{bad}");
        }
    }

    // ---------------------------------------------------------- compact

    #[test]
    fn compact_writes_like_python() {
        let value = jsonio::loads(r#"{"teile": [["a\"b\\c\n", 1, "ä☃"]], "x": 1.5, "y": {}, "z": []}"#).unwrap();
        assert_eq!(
            compact(&value),
            "{\"teile\":[[\"a\\\"b\\\\c\\n\",1,\"\u{e4}\u{2603}\"]],\"x\":1.5,\"y\":{},\"z\":[]}"
        );
    }

    // ---------------------------------------------------------- due

    fn store(geholt: f64, spielstand: Option<&str>) -> Value {
        let mut fields = vec![
            ("teile".to_string(), Value::Array(Vec::new())),
            ("format".to_string(), Value::int(FORMAT)),
            ("geholt".to_string(), Value::Float(geholt)),
        ];
        if let Some(live) = spielstand {
            fields.push(("spielstand".to_string(), Value::str(live)));
        }
        Value::Object(fields)
    }

    #[test]
    fn due_follows_the_patch_not_the_clock() {
        let now = NOW as f64;
        let ago = |days: f64| now - days * DAY;
        // No file.
        assert!(due_from(None, now, "4.10.2"));
        // Stamped and the same build: stays for weeks.
        assert!(!due_from(Some(&store(ago(1.0), Some("4.10.2"))), now, "4.10.2"));
        assert!(!due_from(Some(&store(ago(80.0), Some("4.10.2"))), now, "4.10.2"));
        // The emergency limit.
        assert!(due_from(Some(&store(ago(91.0), Some("4.10.2"))), now, "4.10.2"));
        // A patch since then.
        assert!(due_from(Some(&store(ago(1.0), Some("4.10.1"))), now, "4.10.2"));
        // Live build unknown: no patch is noticed.
        assert!(!due_from(Some(&store(ago(1.0), Some("4.10.1"))), now, ""));
        // Without a stamp it lives one week.
        assert!(!due_from(Some(&store(ago(6.0), None)), now, "4.10.2"));
        assert!(due_from(Some(&store(ago(7.5), None)), now, "4.10.2"));
        // Without a time, or a time that is no number.
        let mut no_time = store(ago(1.0), Some("4.10.2"));
        if let Value::Object(map) = &mut no_time {
            map.retain(|(key, _)| key != "geholt");
        }
        assert!(due_from(Some(&no_time), now, "4.10.2"));
        let mut text_time = store(ago(1.0), Some("4.10.2"));
        jsonio_set(&mut text_time, "geholt", Value::str("soon"));
        assert!(due_from(Some(&text_time), now, "4.10.2"));
    }

    fn jsonio_set(value: &mut Value, key: &str, new: Value) {
        if let Value::Object(map) = value {
            jsonio::map_set(map, key, new);
        }
    }

    #[test]
    fn due_reads_the_file_and_rejects_another_format() {
        let folder = Folder::new();
        assert!(goods_due(&folder.paths, at(NOW), "4.10.2"));
        folder.put(CATALOG_FILE, &compact(&store(NOW as f64 - 60.0, Some("4.10.2"))));
        assert!(!goods_due(&folder.paths, at(NOW), "4.10.2"));
        folder.put(
            CATALOG_FILE,
            &compact(&store(NOW as f64 - 60.0, Some("4.10.2"))).replace("\"format\":1", "\"format\":2"),
        );
        assert!(goods_due(&folder.paths, at(NOW), "4.10.2"));
        folder.put(CATALOG_FILE, "[1, 2]");
        assert!(goods_due(&folder.paths, at(NOW), "4.10.2"));
        folder.put(CATALOG_FILE, "{broken");
        assert!(goods_due(&folder.paths, at(NOW), "4.10.2"));
    }

    // ---------------------------------------------------------- game build

    #[test]
    fn the_live_build_is_asked_for_and_kept_half_a_day() {
        let folder = Folder::new();
        let fake = uex();
        let fetch = |url: &str| fake.fetch(url);
        assert_eq!(live_build(&folder.paths), "");
        assert_eq!(update_build_with(&folder.paths, at(NOW), &fetch), Outcome::Fresh);
        assert_eq!(live_build(&folder.paths), "4.10.2");
        assert_eq!(
            folder.text(BUILD_FILE).unwrap(),
            format!("{{\"live\": \"4.10.2\", \"ptu\": \"\", \"format\": 1, \"geholt\": {NOW}.0}}")
        );
        // Within twelve hours: no new request.
        assert_eq!(update_build_with(&folder.paths, at(NOW + 3600), &fetch), Outcome::Current);
        assert_eq!(fake.calls().len(), 1);
        assert_eq!(update_build_with(&folder.paths, at(NOW + 13 * 3600), &fetch), Outcome::Fresh);
        assert_eq!(fake.calls().len(), 2);
    }

    #[test]
    fn a_bad_answer_about_the_build_keeps_the_old_one() {
        let folder = Folder::new();
        folder.put(BUILD_FILE, "{\"live\": \"4.9.0\", \"ptu\": \"\", \"format\": 1, \"geholt\": 1.0}");
        for answer in [json!([]), json!({ "live": "  " }), json!({ "ptu": "4.11" })] {
            let fake = Fake::new().data(GAME_VERSIONS_URL, answer);
            assert!(matches!(update_build_with(&folder.paths, at(NOW), &|u| fake.fetch(u)), Outcome::Failed(_)));
            assert_eq!(live_build(&folder.paths), "4.9.0");
        }
        let fake = Fake::new().error(GAME_VERSIONS_URL, FetchError::Offline);
        assert_eq!(update_build_with(&folder.paths, at(NOW), &|u| fake.fetch(u)), Outcome::Offline);
        assert_eq!(live_build(&folder.paths), "4.9.0");
    }

    // ---------------------------------------------------------- goods

    #[test]
    fn the_goods_file_is_built_like_4_12() {
        let folder = Folder::new();
        let fake = uex();
        assert_eq!(update_goods_with(&folder.paths, at(NOW), &|url| fake.fetch(url)), Outcome::Fresh);
        assert_eq!(folder.text(CATALOG_FILE).unwrap(), expected_goods("4.10.2", &format!("{NOW}.0")));
        // Only the game related item categories were asked for (the one with
        // the number zero is no category), each once.
        let mut wanted = vec![
            GAME_VERSIONS_URL.to_string(),
            CATEGORIES_URL.to_string(),
            COMMODITIES_URL.to_string(),
            item_url(1),
            item_url(2),
        ];
        wanted.sort();
        assert_eq!(fake.calls(), wanted);
        assert!(folder.leftovers().is_empty());
    }

    #[test]
    fn the_pass_in_the_background_renews_the_build_and_fetches_the_goods_only_when_due() {
        let folder = Folder::new();
        let fake = uex();
        let fetch = |url: &str| fake.fetch(url);
        assert_eq!(
            keep_with(&folder.paths, at(NOW), &fetch),
            Kept { build: Outcome::Fresh, goods: Some(Outcome::Fresh) }
        );
        assert_eq!(live_build(&folder.paths), "4.10.2");
        // A minute later nothing is due and nobody is asked.
        let calls = fake.calls().len();
        assert_eq!(keep_with(&folder.paths, at(NOW + 60), &fetch), Kept { build: Outcome::Current, goods: None });
        assert_eq!(fake.calls().len(), calls);
        // Half a day later the build is asked for again; the same build leaves the goods as they are.
        assert_eq!(keep_with(&folder.paths, at(NOW + 13 * 3600), &fetch), Kept { build: Outcome::Fresh, goods: None });
        assert_eq!(fake.calls().len(), calls + 1);
        // A patch: the build of the running game is kept, and the goods are fetched anew under it.
        let patched = uex().data(GAME_VERSIONS_URL, json!({ "live": "4.11.0", "ptu": null }));
        let later = at(NOW + 26 * 3600);
        let kept = keep_with(&folder.paths, later, &|url| patched.fetch(url));
        assert_eq!(kept, Kept { build: Outcome::Fresh, goods: Some(Outcome::Fresh) });
        assert!(!kept.refused());
        assert_eq!(live_build(&folder.paths), "4.11.0");
        assert_eq!(folder.text(CATALOG_FILE).unwrap(), expected_goods("4.11.0", &format!("{}.0", NOW + 26 * 3600)));
    }

    #[test]
    fn a_list_fetched_before_the_patch_shows_the_older_build_once_the_pass_renewed_it() {
        let folder = Folder::new();
        let fake = uex();
        keep_with(&folder.paths, at(NOW), &|url| fake.fetch(url));
        // The prices for selling, stamped with the build of their fetch.
        folder.put(
            crate::selling::CACHE,
            &format!(r#"{{"terminals": {{}}, "waren": {{}}, "format": 5, "geholt": {NOW}.0, "spielstand": "4.10.2"}}"#),
        );
        let selling = crate::selling::Selling::load(&folder.paths);
        assert_eq!(selling.build, live_build(&folder.paths), "same build: no hint");
        let patched = uex().data(GAME_VERSIONS_URL, json!({ "live": "4.11.0", "ptu": null }));
        keep_with(&folder.paths, at(NOW + 13 * 3600), &|url| patched.fetch(url));
        assert_eq!(crate::selling::Selling::load(&folder.paths).build, "4.10.2");
        assert_eq!(live_build(&folder.paths), "4.11.0", "the page compares the list with the running game");
    }

    #[test]
    fn a_write_the_lock_refuses_is_told_by_the_pass() {
        let folder = Folder::new();
        let fake = uex();
        crate::writeguard::deny(true);
        let kept = keep_with(&folder.paths, at(NOW), &|url| fake.fetch(url));
        crate::writeguard::deny(false);
        let _ = crate::writeguard::take_denied();
        assert!(kept.refused(), "{kept:?}");
        assert_eq!(live_build(&folder.paths), "");
    }

    #[test]
    fn a_fresh_file_asks_for_nothing_but_the_build() {
        let folder = Folder::new();
        let fake = uex();
        let fetch = |url: &str| fake.fetch(url);
        assert_eq!(update_goods_with(&folder.paths, at(NOW), &fetch), Outcome::Fresh);
        let before = folder.text(CATALOG_FILE);
        let calls = fake.calls().len();
        // Next day: the build is asked for again (older than twelve hours),
        // the goods are not.
        assert_eq!(update_goods_with(&folder.paths, at(NOW + 86_400), &fetch), Outcome::Current);
        assert_eq!(fake.calls().len(), calls + 1);
        assert_eq!(folder.text(CATALOG_FILE), before);
    }

    #[test]
    fn a_new_patch_fetches_the_goods_again() {
        let folder = Folder::new();
        let fake = uex();
        assert_eq!(update_goods_with(&folder.paths, at(NOW), &|url| fake.fetch(url)), Outcome::Fresh);
        let patched = Fake { answers: HashMap::new(), calls: Mutex::new(Vec::new()) };
        let mut patched = patched.data(GAME_VERSIONS_URL, json!({ "live": "4.11.0", "ptu": "4.12.0-ptu" }));
        patched.answers.extend(uex().answers.into_iter().filter(|(url, _)| url != GAME_VERSIONS_URL));
        assert_eq!(update_goods_with(&folder.paths, at(NOW + 86_400), &|url| patched.fetch(url)), Outcome::Fresh);
        let text = folder.text(CATALOG_FILE).unwrap();
        assert!(text.ends_with(&format!("\"geholt\":{}.0,\"spielstand\":\"4.11.0\"}}", NOW + 86_400)), "{text}");
    }

    #[test]
    fn without_a_known_build_the_file_has_no_stamp_and_lives_a_week() {
        let folder = Folder::new();
        let mut fake = uex();
        fake.answers.remove(GAME_VERSIONS_URL);
        let fetch = |url: &str| fake.fetch(url);
        assert_eq!(update_goods_with(&folder.paths, at(NOW), &fetch), Outcome::Fresh);
        let text = folder.text(CATALOG_FILE).unwrap();
        assert!(text.ends_with(&format!("\"geholt\":{NOW}.0}}")), "{text}");
        assert!(!text.contains("spielstand"));
        assert_eq!(update_goods_with(&folder.paths, at(NOW + 6 * 86_400), &fetch), Outcome::Current);
        assert_eq!(update_goods_with(&folder.paths, at(NOW + 8 * 86_400), &fetch), Outcome::Fresh);
    }

    #[test]
    fn a_missing_category_leaves_the_old_file_untouched() {
        let folder = Folder::new();
        let old = compact(&store(1.0, Some("4.9.0")));
        folder.put(CATALOG_FILE, &old);
        let fake = uex().error(&item_url(2), FetchError::Failed("HTTP 500".into()));
        let outcome = update_goods_with(&folder.paths, at(NOW), &|url| fake.fetch(url));
        assert_eq!(outcome, Outcome::Failed(Reason::Fetch(Source::Items(2), FetchError::Failed("HTTP 500".into()))));
        assert_eq!(folder.text(CATALOG_FILE).as_deref(), Some(old.as_str()));
        assert!(folder.leftovers().is_empty());
        // Without an old file nothing appears either.
        let empty = Folder::new();
        let fake = uex().error(&item_url(1), FetchError::Refused);
        assert!(matches!(update_goods_with(&empty.paths, at(NOW), &|url| fake.fetch(url)), Outcome::Failed(_)));
        assert_eq!(empty.text(CATALOG_FILE), None);
    }

    #[test]
    fn answers_that_prove_nothing_fail() {
        let cases: Vec<(&str, Fake)> = vec![
            ("no categories", uex().data(CATEGORIES_URL, json!([]))),
            ("categories without data", uex().raw(CATEGORIES_URL, b"{\"status\":\"ok\"}")),
            ("categories no list", uex().data(CATEGORIES_URL, json!({ "a": 1 }))),
            ("categories no json", uex().raw(CATEGORIES_URL, b"<html>")),
            ("categories no object", uex().raw(CATEGORIES_URL, b"[1]")),
            (
                "bad category number",
                uex().data(CATEGORIES_URL, json!([{ "id": "x", "type": "item", "is_game_related": 1 }])),
            ),
            ("no commodities", uex().data(COMMODITIES_URL, json!([]))),
            ("commodities failed", uex().error(COMMODITIES_URL, FetchError::Missing)),
            (
                "nothing at all",
                uex().data(&item_url(1), json!([])).data(&item_url(2), json!([])).data(COMMODITIES_URL, json!([{}])),
            ),
        ];
        for (what, fake) in cases {
            let folder = Folder::new();
            let outcome = update_goods_with(&folder.paths, at(NOW), &|url| fake.fetch(url));
            assert!(matches!(outcome, Outcome::Failed(_)), "{what}: {outcome:?}");
            assert_eq!(folder.text(CATALOG_FILE), None, "{what}");
        }
    }

    #[test]
    fn an_empty_group_is_no_failure() {
        let folder = Folder::new();
        let fake = uex().data(&item_url(1), json!([]));
        assert_eq!(update_goods_with(&folder.paths, at(NOW), &|url| fake.fetch(url)), Outcome::Fresh);
        assert!(folder.text(CATALOG_FILE).unwrap().starts_with("{\"teile\":[[\"Archimedes & Merlin Livery\",4245,"));
    }

    #[test]
    fn the_network_switch_stops_everything() {
        // The function object of a switched-off program asks for nothing.
        let off = fetcher("VerseKit/0", true);
        assert_eq!(off("https://api.uexcorp.uk/2.0/categories"), Err(FetchError::Offline));
        let folder = Folder::new();
        assert_eq!(update_goods_with(&folder.paths, at(NOW), &off), Outcome::Offline);
        assert_eq!(update_build_with(&folder.paths, at(NOW), &off), Outcome::Offline);
        assert_eq!(update_mining_with(&folder.paths, "4.10.2-live.1", &off), Outcome::Offline);
        assert_eq!(folder.text(CATALOG_FILE), None);
        assert_eq!(folder.text(BUILD_FILE), None);
        assert_eq!(folder.text(MINING_FILE), None);
        // The same when it is the middle of the lists that cannot be asked.
        let fake = uex().error(&item_url(2), FetchError::Offline);
        assert_eq!(update_goods_with(&folder.paths, at(NOW), &|url| fake.fetch(url)), Outcome::Offline);
        assert_eq!(folder.text(CATALOG_FILE), None);
    }

    #[test]
    fn a_write_the_lock_refuses_is_no_error() {
        let folder = Folder::new();
        let fake = uex();
        writeguard::deny(true);
        let goods = update_goods_with(&folder.paths, at(NOW), &|url| fake.fetch(url));
        writeguard::deny(false);
        let _ = writeguard::take_denied();
        assert_eq!(goods, Outcome::Refused);
        assert_eq!(folder.text(CATALOG_FILE), None);
        assert!(folder.leftovers().is_empty());
    }

    #[test]
    fn the_stock_data_reads_the_fetched_goods() {
        let folder = Folder::new();
        let fake = uex();
        assert_eq!(update_goods_with(&folder.paths, at(NOW), &|url| fake.fetch(url)), Outcome::Fresh);
        let data = StockData::load(|name| folder.file(name));
        let item = data.item_choice("venture arms").unwrap();
        assert_eq!(item.name, "Venture Arms");
        assert_eq!(item.item, Some(Value::int(171)));
        assert_eq!(item.material, Some(Value::int(9)));
        assert_eq!(item.section, "Armor");
        let grey = data.item_choice("Grey's Helmet").unwrap();
        assert_eq!(grey.item, Some(Value::int(173)));
        assert_eq!(data.item_choice("Archimedes & Merlin Livery").unwrap().section, "Liveries");
        assert_eq!(data.choices().get("agricium (ore)").unwrap().material, Some(Value::int(2)));
        assert!(data.item_choice("Agricium").is_none());
    }

    // ---------------------------------------------------------- mining

    fn scmdb_file() -> Vec<u8> {
        json!({
            "locations": [{
                "locationName": "Daymar",
                "groups": [{ "groupName": "Harvestables", "deposits": [{ "presetName": "Plant HeartoftheWoods" }] }]
            }],
            "compositions": { "g1": { "name": "Iron", "parts": [] } },
            "refineries": [{ "name": "Refinery" }],
            "refineryProfiles": { "p": { "x": 1.5 } },
            "mineableElements": { "e1": { "name": "Iron (Raw)", "materialName": "Iron" } },
            "ignored": "field"
        })
        .to_string()
        .into_bytes()
    }

    fn scmdb_fake(build: &str) -> Fake {
        Fake::new().raw(&format!("mining_data-{build}.json"), &scmdb_file())
    }

    const BUILD: &str = "4.10.2-live.12913470";

    #[test]
    fn the_mining_file_is_built_like_4_12() {
        let folder = Folder::new();
        let fake = scmdb_fake(BUILD);
        assert_eq!(update_mining_with(&folder.paths, BUILD, &|name| fake.fetch(name)), Outcome::Fresh);
        assert_eq!(fake.calls(), vec![format!("mining_data-{BUILD}.json")]);
        let text = folder.text(MINING_FILE).unwrap();
        let value = jsonio::loads(&text).unwrap();
        let keys: Vec<&str> = value.as_object().unwrap().iter().map(|(key, _)| key.as_str()).collect();
        assert_eq!(
            keys,
            ["format", "build", "locations", "compositions", "refineries", "refineryProfiles", "elemente"]
        );
        assert!(text.starts_with(&format!("{{\"format\": 1, \"build\": \"{BUILD}\", \"locations\": [{{")), "{text}");
        assert!(text.contains("\"elemente\": {\"e1\": {"));
        assert!(text.contains("\"refineryProfiles\": {\"p\": {\"x\": 1.5}}"));
        assert!(!text.contains("ignored"));
        assert!(folder.leftovers().is_empty());
    }

    #[test]
    fn the_stock_data_reads_the_fetched_mining_data() {
        let folder = Folder::new();
        let fake = scmdb_fake(BUILD);
        assert_eq!(update_mining_with(&folder.paths, BUILD, &|name| fake.fetch(name)), Outcome::Fresh);
        let data = StockData::load(|name| folder.file(name));
        // The element list gives the mineral, the plant comes from the
        // harvestable deposit.
        assert_eq!(data.storable(), ["Heart of the Woods", "Iron (Raw)"]);
        assert_eq!(data.material_kind("Iron", false), crate::stock_data::Kind::Refined);
        assert_eq!(data.material_kind("Heart of the Woods", false), crate::stock_data::Kind::Plant);
    }

    #[test]
    fn the_same_build_is_not_fetched_again() {
        let folder = Folder::new();
        let fake = scmdb_fake(BUILD);
        let fetch = |name: &str| fake.fetch(name);
        assert!(!mining_current(&folder.paths, BUILD));
        assert_eq!(update_mining_with(&folder.paths, BUILD, &fetch), Outcome::Fresh);
        assert!(mining_current(&folder.paths, BUILD));
        assert_eq!(update_mining_with(&folder.paths, BUILD, &fetch), Outcome::Current);
        assert_eq!(fake.calls().len(), 1);
        // Another build, or a file without refineries or elements, is fetched.
        assert!(!mining_current(&folder.paths, "4.11.0-live.1"));
        let old = folder.text(MINING_FILE).unwrap();
        for missing in ["refineries", "elemente"] {
            folder.put(MINING_FILE, &old.replace(&format!("\"{missing}\":"), "\"other\":"));
            assert!(!mining_current(&folder.paths, BUILD), "{missing}");
            assert_eq!(update_mining_with(&folder.paths, BUILD, &fetch), Outcome::Fresh);
        }
        // A null section counts as missing, an empty list does not.
        folder.put(MINING_FILE, &old.replace("\"refineries\": [{\"name\": \"Refinery\"}]", "\"refineries\": null"));
        assert!(!mining_current(&folder.paths, BUILD));
        folder.put(MINING_FILE, &old.replace("\"refineries\": [{\"name\": \"Refinery\"}]", "\"refineries\": []"));
        assert!(mining_current(&folder.paths, BUILD));
        // No locations: fetched again.
        folder.put(MINING_FILE, &old.replacen("\"locations\": [", "\"locations\": [], \"x\": [", 1));
        assert!(!mining_current(&folder.paths, BUILD));
    }

    #[test]
    fn a_bad_mining_answer_keeps_the_old_file() {
        let folder = Folder::new();
        let old = "{\"format\": 1, \"build\": \"old\", \"locations\": [1]}";
        folder.put(MINING_FILE, old);
        let cases: Vec<(&str, Fake)> = vec![
            ("no locations", Fake::new().raw(&format!("mining_data-{BUILD}.json"), b"{\"locations\": []}")),
            ("no json", Fake::new().raw(&format!("mining_data-{BUILD}.json"), b"<html>")),
            ("no object", Fake::new().raw(&format!("mining_data-{BUILD}.json"), b"[]")),
            ("missing", Fake::new().error(&format!("mining_data-{BUILD}.json"), FetchError::Missing)),
        ];
        for (what, fake) in cases {
            let outcome = update_mining_with(&folder.paths, BUILD, &|name| fake.fetch(name));
            assert!(matches!(outcome, Outcome::Failed(_)), "{what}: {outcome:?}");
            assert_eq!(folder.text(MINING_FILE).as_deref(), Some(old), "{what}");
        }
        assert_eq!(
            update_mining_with(&folder.paths, "", &|_| Err(FetchError::Missing)),
            Outcome::Failed(Reason::NoBuild)
        );
        assert!(folder.leftovers().is_empty());
    }

    #[test]
    fn a_mining_write_the_lock_refuses_is_no_error() {
        let folder = Folder::new();
        let fake = scmdb_fake(BUILD);
        writeguard::deny(true);
        let outcome = update_mining_with(&folder.paths, BUILD, &|name| fake.fetch(name));
        writeguard::deny(false);
        let _ = writeguard::take_denied();
        assert_eq!(outcome, Outcome::Refused);
        assert_eq!(folder.text(MINING_FILE), None);
    }

    // ---------------------------------------------------------- 4.12 files

    /// The data folder of a 4.12 installation (`VERSEKIT_PYTHON_DATA`, its
    /// `Intern` folder), when the test machine has one.
    fn python_data() -> Option<String> {
        std::env::var("VERSEKIT_PYTHON_DATA").ok().filter(|dir| fs::metadata(dir).is_ok())
    }

    #[test]
    fn the_writers_reproduce_the_files_of_4_12_byte_for_byte() {
        let Some(dir) = python_data() else { return };
        let goods = fs::read_to_string(format!("{dir}/{CATALOG_FILE}")).unwrap();
        assert_eq!(compact(&jsonio::loads(&goods).unwrap()), goods);
        let mining = fs::read_to_string(format!("{dir}/{MINING_FILE}")).unwrap();
        assert_eq!(jsonio::dumps(&jsonio::loads(&mining).unwrap(), None, false), mining);
        let build = fs::read_to_string(format!("{dir}/{BUILD_FILE}")).unwrap();
        assert_eq!(jsonio::dumps(&jsonio::loads(&build).unwrap(), None, false), build);
    }

    /// Fetches the real lists. Needs a network; run with `--ignored`.
    #[test]
    #[ignore = "needs the network"]
    fn live_goods_and_build() {
        let folder = Folder::new();
        let fetch = live_fetcher("VerseKit-test");
        assert_eq!(update_build_with(&folder.paths, SystemTime::now(), &fetch), Outcome::Fresh);
        let live = live_build(&folder.paths);
        assert!(!live.is_empty());
        let started = std::time::Instant::now();
        assert_eq!(update_goods_with(&folder.paths, SystemTime::now(), &fetch), Outcome::Fresh);
        let took = started.elapsed();
        let text = folder.text(CATALOG_FILE).unwrap();
        let value = jsonio::loads(&text).unwrap();
        let count = |key: &str| match value.get(key) {
            Some(Value::Array(rows)) => rows.len(),
            _ => 0,
        };
        eprintln!("live {live}: {} parts, {} goods, {:?}", count("teile"), count("waren"), took);
        assert!(count("teile") > 1000 && count("waren") > 50);
        if let Some(dir) = python_data() {
            let theirs = jsonio::loads(&fs::read_to_string(format!("{dir}/{CATALOG_FILE}")).unwrap()).unwrap();
            let rows = |value: &Value, key: &str| match value.get(key) {
                Some(Value::Array(rows)) => rows.iter().map(compact).collect::<Vec<_>>(),
                _ => Vec::new(),
            };
            for key in ["teile", "waren"] {
                let (mine, old) = (rows(&value, key), rows(&theirs, key));
                let new: Vec<_> = mine.iter().filter(|row| !old.contains(row)).collect();
                let gone: Vec<_> = old.iter().filter(|row| !mine.contains(row)).collect();
                eprintln!("{key}: {} now, {} in 4.12, {} new, {} gone", mine.len(), old.len(), new.len(), gone.len());
                eprintln!("  new: {:?}", &new[..new.len().min(10)]);
                eprintln!("  gone: {:?}", &gone[..gone.len().min(10)]);
                // What both have comes in the same order.
                let shared: Vec<&String> = mine.iter().filter(|row| old.contains(row)).collect();
                assert!(shared.iter().copied().eq(old.iter().filter(|row| mine.contains(row))), "{key}: order");
            }
        }
    }

    /// Fetches the real mining file of the build in `VERSEKIT_TEST_BUILD`.
    #[test]
    #[ignore = "needs the network"]
    fn live_mining() {
        let Ok(build) = std::env::var("VERSEKIT_TEST_BUILD") else { return };
        let folder = Folder::new();
        assert_eq!(update_mining(&folder.paths, &build, "VerseKit-test"), Outcome::Fresh);
        let mine = folder.text(MINING_FILE).unwrap();
        eprintln!("mining file: {} bytes", mine.len());
        if let Some(dir) = python_data()
            && let Ok(theirs) = fs::read_to_string(format!("{dir}/{MINING_FILE}"))
            && jsonio::loads(&theirs).ok().and_then(|v| v.get("build").and_then(Value::as_str).map(str::to_string))
                == Some(build.clone())
        {
            eprintln!("identical to 4.12: {}", mine == theirs);
        }
    }
}
