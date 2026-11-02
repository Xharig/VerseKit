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

//! The own stock: materials for crafting (`rohstoffe.json`, `materials.py`)
//! and goods and items for sale (`handelslager.json`, `trade_cargo.py`),
//! plus the number input both share.
//!
//! ⚠ Stored keys stay German (`format`, `posten`, `material`, `menge`,
//! `qualitaet`, `ort`, `gestohlen`, `ware`, `stueck`, `einkauf`, `uex`).
//! Entries are JSON objects; an operation changes one entry the way the
//! Python version does, all others stay as they were read.
//!
//! ⚠ An SCU entry is one crate: equal material, quality and place are not
//! merged. Only pieces (gems) of equal quality at the same place add up.

use crate::errors;
use crate::jsonio::{self, Map, Value, map_set};
use crate::language::{Arg, Lang, py_g, tf_in};
use crate::stock_data::{StockData, decimal_digit, format_is, py_float, py_float_value, py_split, text_of, truthy};
use crate::text::py_strip;

static STOCK_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Hold while reading, changing and writing the stock files as one step: the page and the sync write
/// them from different threads. Not to be taken twice in one thread.
pub fn lock() -> std::sync::MutexGuard<'static, ()> {
    STOCK_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

pub const RAW_FILE: &str = "rohstoffe.json";
pub const TRADE_FILE: &str = "handelslager.json";
pub const FORMAT: i64 = 1;
/// SCU per cSCU; the refinery terminal counts in cSCU.
pub const CSCU: f64 = 0.01;
/// From this many SCU on, a mix-up with cSCU is more likely than the amount.
pub const SCU_SUSPICIOUS: f64 = 100.0;

// ------------------------------------------------------------ numbers

/// Python's `round(x, places)`.
pub fn py_round(x: f64, places: usize) -> f64 {
    if !x.is_finite() {
        return x;
    }
    format!("{x:.places$}").parse().unwrap_or(x)
}

/// Python's `round(x)` to an integer (ties to even).
pub fn py_round_int(x: f64) -> i64 {
    x.round_ties_even() as i64
}

/// Collapse a run of `d[.,]ddd` groups (`(\d)[.,](\d{3})(?!\d)` replaced by
/// `\1\2`).
fn drop_thousands(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let digit = |k: usize| chars.get(i + k).is_some_and(|c| decimal_digit(*c).is_some());
        let group = digit(0) && matches!(chars.get(i + 1), Some('.' | ',')) && (2..5).all(digit) && !digit(5);
        if group {
            out.push(c);
            out.extend(&chars[i + 2..i + 5]);
            i += 5;
        } else {
            out.push(c);
            i += 1;
        }
    }
    out
}

/// Resolve thousands separators where unambiguous and switch to the dot
/// (`materials.normalize_separators`).
pub fn normalize_separators(text: &str, integer: bool) -> String {
    let raw = py_strip(text);
    if raw.is_empty() {
        return String::new();
    }
    let commas = raw.matches(',').count();
    let dots = raw.matches('.').count();
    if commas > 0 && dots > 0 {
        let decimal = if raw.rfind(',') > raw.rfind('.') { ',' } else { '.' };
        let other = if decimal == ',' { '.' } else { ',' };
        return raw.replace(other, "").replace(',', ".");
    }
    let mut raw = raw.to_string();
    if integer || commas > 1 || dots > 1 {
        loop {
            let next = drop_thousands(&raw);
            if next == raw {
                break;
            }
            raw = next;
        }
    }
    raw.replace(',', ".")
}

/// A typed number; comma and dot count the same (`materials.parse_number`).
pub fn parse_number(text: &str) -> Option<f64> {
    let raw = normalize_separators(text, false).replace('−', "-");
    if raw.is_empty() {
        return None;
    }
    py_float(&raw)
}

fn is_number_text(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| decimal_digit(c).is_some() || c == '.' || c == ',')
}

fn trim_regex_space(s: &str) -> &str {
    s.trim_matches(char::is_whitespace)
}

/// What the amount field says, as a number (`materials.calculate`):
/// `4,5` is the new amount, `+3`/`-3` books onto `previous`, `1,04+3` is
/// worked out.
pub fn calculate(text: &str, previous: f64) -> Option<f64> {
    let raw = py_strip(text);
    if raw.is_empty() {
        return None;
    }
    let body = trim_regex_space(raw);
    let mut chars = body.chars();
    if let Some(sign @ ('+' | '-' | '−')) = chars.next() {
        let rest = trim_regex_space(chars.as_str());
        if is_number_text(rest) {
            let value = parse_number(rest)?;
            return Some(previous + if sign == '+' { value } else { -value });
        }
    }
    if let Some(pos) = body.char_indices().skip(1).find(|(_, c)| matches!(c, '+' | '-')).map(|(i, _)| i) {
        let (left, right) = (trim_regex_space(&body[..pos]), trim_regex_space(&body[pos + 1..]));
        if is_number_text(left) && is_number_text(right) {
            let (left, right) = (parse_number(left)?, parse_number(right)?);
            return Some(if body[pos..].starts_with('-') { left - right } else { left + right });
        }
    }
    parse_number(raw)
}

/// Does the field hold a calculation (a sign anywhere)?
pub fn is_calculation(text: &str) -> bool {
    let raw = py_strip(text);
    let mut chars = raw.chars();
    match chars.next() {
        None => false,
        Some(first) => matches!(first, '+' | '-' | '−') || chars.any(|c| matches!(c, '+' | '-' | '−')),
    }
}

/// Text key of a hint when the unit looks mixed up (`materials.unit_hint`).
pub fn unit_hint(value: f64, cscu: bool) -> Option<&'static str> {
    if !cscu && value >= SCU_SUSPICIOUS {
        return Some("s_lg_hinweis_scu_gross");
    }
    if cscu && (value - value.round_ties_even()).abs() > 1e-9 {
        return Some("s_lg_hinweis_cscu_komma");
    }
    None
}

/// A stock amount for display, without unit (`materials.amount_text`): SCU
/// on three places like the game, whole numbers bare; pieces as `%g`.
pub fn amount_text(value: f64, piece: bool, decimal: char) -> String {
    let value = if value.is_finite() && value != 0.0 { value } else { 0.0 };
    if piece {
        return py_g(value, false).replace('.', &decimal.to_string());
    }
    let rounded = py_round(value, 3);
    if rounded == rounded.trunc() {
        return format!("{}", rounded as i64);
    }
    format!("{rounded:.3}").replace('.', &decimal.to_string())
}

// ------------------------------------------------------------ files

/// All entries of a stock file, or none (`materials.load`, `trade_cargo.load`).
pub fn load(path: &str, report_as: &str) -> Vec<Value> {
    let valid = |v: &Value| matches!(v, Value::Object(_));
    match jsonio::load_json(path, Some(&valid), report_as) {
        Some(data) if format_is(&data, FORMAT) => match data.get("posten") {
            Some(Value::Array(entries)) => entries.clone(),
            _ => Vec::new(),
        },
        _ => Vec::new(),
    }
}

/// Like [`load`], but a file that is there and cannot be read as a stock gives
/// `None` instead of an empty list. For the sync, which must never take a
/// stock for empty because it could not be read once.
pub fn load_checked(path: &str, report_as: &str) -> Option<Vec<Value>> {
    if !std::path::Path::new(path).exists() {
        return Some(Vec::new());
    }
    let valid = |v: &Value| matches!(v, Value::Object(_));
    match jsonio::load_json(path, Some(&valid), report_as) {
        Some(data) if format_is(&data, FORMAT) => match data.get("posten") {
            Some(Value::Array(entries)) => Some(entries.clone()),
            _ => None,
        },
        _ => None,
    }
}

/// The file content as Python writes it.
pub fn file_value(entries: &[Value]) -> Value {
    Value::Object(vec![("format".into(), Value::int(FORMAT)), ("posten".into(), Value::Array(entries.to_vec()))])
}

/// Write the entries atomically with a previous-version backup; a failure
/// is reported.
pub fn save(path: &str, entries: &[Value], report_as: &str) -> bool {
    match jsonio::save_json(path, &file_value(entries), 1, false) {
        Ok(()) => {
            crate::basetool::sync::store_saved(path);
            true
        }
        Err(e) => {
            errors::record(report_as, &e);
            false
        }
    }
}

fn get<'a>(entry: &'a Value, key: &str) -> Option<&'a Value> {
    entry.get(key)
}

/// `float(entry.get(key) or 0)`, `0.0` where that fails.
pub fn number_of(entry: &Value, key: &str) -> f64 {
    let v = get(entry, key);
    if !truthy(v) {
        return 0.0;
    }
    v.and_then(py_float_value).unwrap_or(0.0)
}

fn object_mut(entry: &mut Value) -> Option<&mut Map> {
    match entry {
        Value::Object(m) => Some(m),
        _ => None,
    }
}

fn pop(entry: &mut Value, key: &str) {
    if let Some(m) = object_mut(entry) {
        m.retain(|(k, _)| k != key);
    }
}

fn set(entry: &mut Value, key: &str, value: Value) {
    if let Some(m) = object_mut(entry) {
        map_set(m, key, value);
    }
}

fn object(pairs: Vec<(&str, Value)>) -> Value {
    Value::Object(pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
}

fn quality_value(quality: Option<i64>) -> Value {
    quality.map_or(Value::Null, Value::int)
}

// ------------------------------------------------------------ materials

/// The same stack: material, place (without case) and quality
/// (`materials.same_stack`).
fn same_material_stack(material: &str, quality: Option<i64>, place: &str, b: &Value) -> bool {
    use crate::stock_data::{lower, norm_material};
    if norm_material(material) != norm_material(text_of(get(b, "material"))) {
        return false;
    }
    if lower(py_strip(place)) != lower(py_strip(text_of(get(b, "ort")))) {
        return false;
    }
    let other = match get(b, "qualitaet") {
        None | Some(Value::Null) => None,
        Some(v) => py_float_value(v).map(py_round_int),
    };
    quality == other
}

/// Add an entry; pieces add onto an equal stack (`materials.add`).
pub fn raw_add(
    entries: &mut Vec<Value>,
    data: &StockData,
    material: &str,
    amount: f64,
    quality: Option<i64>,
    place: &str,
) {
    if data.is_piece(material)
        && let Some(stack) = entries.iter_mut().find(|p| same_material_stack(material, quality, place, p))
    {
        let total = py_round(number_of(stack, "menge") + amount, 6);
        set(stack, "menge", Value::Float(total));
        return;
    }
    entries.push(object(vec![
        ("material", Value::str(py_strip(material))),
        ("menge", Value::Float(amount)),
        ("qualitaet", quality_value(quality)),
        ("ort", Value::str(py_strip(place))),
    ]));
}

/// Overwrite an entry, keeping its stolen mark (`materials.change`).
pub fn raw_change(
    entries: &mut [Value],
    index: usize,
    material: &str,
    amount: f64,
    quality: Option<i64>,
    place: &str,
) -> bool {
    let Some(slot) = entries.get_mut(index) else { return false };
    let stolen = truthy(get(slot, "gestohlen"));
    let mut entry = object(vec![
        ("material", Value::str(py_strip(material))),
        ("menge", Value::Float(amount)),
        ("qualitaet", quality_value(quality)),
        ("ort", Value::str(py_strip(place))),
    ]);
    if stolen {
        set(&mut entry, "gestohlen", Value::Bool(true));
    }
    *slot = entry;
    true
}

/// Too little left in this crate for any recipe? (`materials.is_leftover`)
pub fn is_leftover(entry: &Value, data: &StockData) -> bool {
    let material = text_of(get(entry, "material"));
    if data.is_piece(material) {
        return false;
    }
    match data.smallest_portion(material, number_of(entry, "qualitaet")) {
        Some(smallest) => number_of(entry, "menge") + 1e-9 < smallest,
        None => false,
    }
}

// ------------------------------------------------------------ trade

/// Why a trade booking was refused (`trade_cargo` reasons).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refused {
    Goods,
    Amount,
    Price,
    Gone,
}

impl Refused {
    /// The reason word of the Python version.
    pub fn code(self) -> &'static str {
        match self {
            Refused::Goods => "ware",
            Refused::Amount => "menge",
            Refused::Price => "preis",
            Refused::Gone => "weg",
        }
    }
}

/// The price field: keep the stored price or check a new text.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Cost<'a> {
    Keep,
    Text(&'a str),
}

fn check_amount(text: &str, previous: f64) -> Option<f64> {
    calculate(text, previous).filter(|n| *n > 0.0)
}

fn check_cost(text: &str) -> Result<Option<f64>, Refused> {
    if py_strip(text).is_empty() {
        return Ok(None);
    }
    match calculate(text, 0.0) {
        Some(n) if n > 0.0 => Ok(Some(n)),
        _ => Err(Refused::Price),
    }
}

/// The price paid per SCU or piece, or `None` (`trade_cargo.cost_of`).
pub fn cost_of(entry: &Value) -> Option<f64> {
    get(entry, "einkauf").and_then(py_float_value).filter(|v| *v > 0.0)
}

fn set_cost(entry: &mut Value, cost: Option<f64>) {
    match cost {
        None => pop(entry, "einkauf"),
        Some(c) => set(entry, "einkauf", Value::Float(c)),
    }
}

/// Counts in whole pieces (an item)? (`trade_cargo.is_pieces`)
pub fn is_pieces(entry: &Value) -> bool {
    truthy(get(entry, "stueck"))
}

/// The same stack: goods, place and stolen mark, exactly
/// (`trade_cargo.same_stack`).
fn same_trade_stack(goods: &str, place: &str, stolen: bool, b: &Value) -> bool {
    let ware_same = matches!(get(b, "ware"), Some(Value::Str(s)) if s == goods);
    let place_b = get(b, "ort");
    let place_same =
        if truthy(place_b) { matches!(place_b, Some(Value::Str(s)) if s == place) } else { place.is_empty() };
    ware_same && place_same && truthy(get(b, "gestohlen")) == stolen
}

fn mixed_cost(old_amount: f64, old_cost: Option<f64>, new_amount: f64, new_cost: Option<f64>) -> Option<f64> {
    let (old_cost, new_cost) = (old_cost?, new_cost?);
    let total = old_amount + new_amount;
    if total <= 0.0 {
        return Some(new_cost);
    }
    Some((old_amount * old_cost + new_amount * new_cost) / total)
}

/// Book goods; equal stacks add up (`trade_cargo.add`).
pub fn trade_add(
    entries: &mut Vec<Value>,
    goods: &str,
    amount: &str,
    place: &str,
    stolen: bool,
    cost: &str,
) -> Result<(), Refused> {
    let goods = py_strip(goods);
    if goods.is_empty() {
        return Err(Refused::Goods);
    }
    let number = check_amount(amount, 0.0).ok_or(Refused::Amount)?;
    let cost = check_cost(cost)?;
    if let Some(stack) = entries.iter_mut().find(|p| same_trade_stack(goods, place, stolen, p)) {
        let before = number_of(stack, "menge");
        let mixed = mixed_cost(before, cost_of(stack), number, cost);
        set_cost(stack, mixed);
        set(stack, "menge", Value::Float(before + number));
        return Ok(());
    }
    let mut entry = object(vec![
        ("ware", Value::str(goods)),
        ("menge", Value::Float(number)),
        ("ort", Value::str(place)),
        ("gestohlen", Value::Bool(stolen)),
    ]);
    set_cost(&mut entry, cost);
    entries.push(entry);
    Ok(())
}

/// Book an item in whole pieces, or overwrite the entry at `index`
/// (`trade_cargo.add_item`).
pub fn trade_add_item(
    entries: &mut Vec<Value>,
    name: &str,
    amount: &str,
    place: &str,
    stolen: bool,
    uex: Option<i64>,
    index: Option<usize>,
) -> Result<(), Refused> {
    let name = py_strip(name);
    if name.is_empty() {
        return Err(Refused::Goods);
    }
    if index.is_some_and(|i| i >= entries.len()) {
        return Err(Refused::Gone);
    }
    let before = index.and_then(|i| entries.get(i)).map_or(0.0, |e| number_of(e, "menge"));
    let number = check_amount(amount, before).filter(|n| *n >= 1.0).ok_or(Refused::Amount)?;
    let pieces = py_round_int(number) as f64;
    let at = match index {
        Some(i) => {
            let entry = &mut entries[i];
            set(entry, "ware", Value::str(name));
            set(entry, "menge", Value::Float(pieces));
            set(entry, "ort", Value::str(place));
            set(entry, "gestohlen", Value::Bool(stolen));
            set(entry, "stueck", Value::Bool(true));
            i
        }
        None => match entries.iter().position(|p| is_pieces(p) && same_trade_stack(name, place, stolen, p)) {
            Some(i) => {
                let total = number_of(&entries[i], "menge") + pieces;
                set(&mut entries[i], "menge", Value::Float(total));
                i
            }
            None => {
                entries.push(object(vec![
                    ("ware", Value::str(name)),
                    ("menge", Value::Float(pieces)),
                    ("ort", Value::str(place)),
                    ("gestohlen", Value::Bool(stolen)),
                    ("stueck", Value::Bool(true)),
                ]));
                entries.len() - 1
            }
        },
    };
    match uex.filter(|n| *n > 0) {
        Some(n) => set(&mut entries[at], "uex", Value::int(n)),
        None if index.is_some() => pop(&mut entries[at], "uex"),
        None => {}
    }
    Ok(())
}

/// Book items in whole pieces with the price paid for one piece. Equal stacks (name, place, stolen mark) add
/// up and their price becomes the average per piece; a stack without a price stays without one, as the
/// average is not known then. The price keeps four decimals, a fraction when a total did not divide.
///
/// `trade_add_item` stays as it is; this one adds the price on top of it.
pub fn trade_add_item_with_cost(
    entries: &mut Vec<Value>,
    name: &str,
    amount: &str,
    place: &str,
    stolen: bool,
    uex: Option<i64>,
    cost: f64,
) -> Result<(), Refused> {
    if !cost.is_finite() || cost <= 0.0 {
        return Err(Refused::Price);
    }
    let stripped = py_strip(name);
    let stack = entries.iter().position(|p| is_pieces(p) && same_trade_stack(stripped, place, stolen, p));
    let before = stack.map(|i| (number_of(&entries[i], "menge"), cost_of(&entries[i])));
    let length = entries.len();
    trade_add_item(entries, name, amount, place, stolen, uex, None)?;
    let at = stack.unwrap_or(length);
    let added = number_of(&entries[at], "menge") - before.map_or(0.0, |(old, _)| old);
    let paid = match before {
        None => Some(cost),
        Some((old_amount, old_cost)) => mixed_cost(old_amount, old_cost, added, Some(cost)),
    };
    set_cost(&mut entries[at], paid.map(|c| py_round(c, 4)));
    Ok(())
}

/// Overwrite a goods entry; `+5` books onto its amount (`trade_cargo.change`).
pub fn trade_change(
    entries: &mut [Value],
    index: usize,
    goods: &str,
    amount: &str,
    place: &str,
    stolen: bool,
    cost: Cost<'_>,
) -> Result<(), Refused> {
    let Some(old) = entries.get(index) else { return Err(Refused::Gone) };
    let goods = py_strip(goods);
    if goods.is_empty() {
        return Err(Refused::Goods);
    }
    let pieces = is_pieces(old);
    let mut number = check_amount(amount, number_of(old, "menge"));
    if pieces {
        number = number.map(py_round_int).filter(|n| *n != 0).map(|n| n as f64);
    }
    let number = number.ok_or(Refused::Amount)?;
    let cost = match cost {
        Cost::Keep => cost_of(old),
        Cost::Text(text) => check_cost(text)?,
    };
    let mut entry = object(vec![
        ("ware", Value::str(goods)),
        ("menge", Value::Float(number)),
        ("ort", Value::str(place)),
        ("gestohlen", Value::Bool(stolen)),
    ]);
    if pieces {
        set(&mut entry, "stueck", Value::Bool(true));
    }
    set_cost(&mut entry, cost);
    entries[index] = entry;
    Ok(())
}

/// Remove the goods, keep the items; the number removed
/// (`trade_cargo.clear_goods`).
pub fn clear_goods(entries: &mut Vec<Value>) -> usize {
    let before = entries.len();
    entries.retain(is_pieces);
    before - entries.len()
}

/// Remove the items, keep the goods (`trade_cargo.clear_items`).
pub fn clear_items(entries: &mut Vec<Value>) -> usize {
    let before = entries.len();
    entries.retain(|p| !is_pieces(p));
    before - entries.len()
}

// ------------------------------------------------------------ backup and table

/// A stock as JSON text, the format `load` reads (`materials.as_json`,
/// `trade_cargo.as_json`).
pub fn as_json(entries: &[Value]) -> String {
    jsonio::dumps(&file_value(entries), Some(1), false)
}

/// Python's `str(value)` for the values a stock file holds; `None` for a
/// list or an object.
fn py_str(v: &Value) -> Option<String> {
    match v {
        Value::Str(s) => Some(s.clone()),
        Value::Int(n) => Some(n.clone()),
        Value::Float(x) => Some(jsonio::float_text(*x)),
        Value::Bool(b) => Some(if *b { "True" } else { "False" }.to_string()),
        Value::Null => Some("None".to_string()),
        _ => None,
    }
}

/// `(entry.get(key) or '')` where a method of text follows: the text, `""`
/// when falsy, `None` where Python raises (a truthy value that is no text).
fn text_or_empty<'a>(entry: &'a Value, key: &str) -> Option<&'a str> {
    match get(entry, key) {
        v if !truthy(v) => Some(""),
        Some(Value::Str(s)) => Some(s),
        _ => None,
    }
}

/// `float(entry.get(key) or 0)`, `None` where Python raises.
fn float_or_zero(entry: &Value, key: &str) -> Option<f64> {
    match get(entry, key) {
        v if !truthy(v) => Some(0.0),
        Some(v) => py_float_value(v),
        None => Some(0.0),
    }
}

/// A CSV cell as 4.12 writes it: `;` inside becomes `,`.
fn csv_cell(text: &str) -> String {
    text.replace(';', ",")
}

/// `'%g' % amount` with a decimal comma.
fn csv_amount(amount: f64) -> String {
    py_g(amount, false).replace('.', ",")
}

/// The materials as a table: `;` separated, decimal comma
/// (`materials.as_csv`); `None` where 4.12 raises on an entry.
pub fn raw_csv(entries: &[Value]) -> Option<String> {
    let mut lines = vec!["Material;Menge;Qualitaet;Lagerort".to_string()];
    for p in entries {
        if !matches!(p, Value::Object(_)) {
            return None;
        }
        let amount = csv_amount(float_or_zero(p, "menge")?);
        let quality = if truthy(get(p, "qualitaet")) {
            py_g(py_float_value(get(p, "qualitaet")?)?, false)
        } else {
            String::new()
        };
        let material = csv_cell(text_or_empty(p, "material")?);
        let place = csv_cell(text_or_empty(p, "ort")?);
        lines.push([material, amount, quality, place].join(";"));
    }
    Some(lines.join("\n") + "\n")
}

/// The goods and items as a table (`trade_cargo.as_csv`); stolen as `ja`.
pub fn trade_csv(entries: &[Value]) -> Option<String> {
    let mut lines = vec!["Ware;Menge;Gestohlen;Lagerort".to_string()];
    for p in entries {
        if !matches!(p, Value::Object(_)) {
            return None;
        }
        let goods = csv_cell(text_or_empty(p, "ware")?);
        let amount = csv_amount(float_or_zero(p, "menge")?);
        let stolen = if truthy(get(p, "gestohlen")) { "ja" } else { "" }.to_string();
        let place = csv_cell(text_or_empty(p, "ort")?);
        lines.push([goods, amount, stolen, place].join(";"));
    }
    Some(lines.join("\n") + "\n")
}

/// The entry list of a backup file, or `None` (no object, other format, no list).
fn backup_entries(text: &str) -> Option<Vec<Value>> {
    let data = jsonio::loads(text).ok()?;
    if !matches!(data, Value::Object(_)) || !format_is(&data, FORMAT) {
        return None;
    }
    match data.get("posten") {
        Some(Value::Array(entries)) => Some(entries.clone()),
        _ => None,
    }
}

/// Where 4.12 raises instead of answering (a value of the wrong type).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Raises;

/// A materials backup read back, cleaned; nothing is saved
/// (`materials.from_json`). `None` where the file does not fit or 4.12 raises.
pub fn raw_from_json(text: &str) -> Option<Vec<Value>> {
    try_raw_from_json(text).ok().flatten()
}

/// [`raw_from_json`] telling a file that does not fit (`Ok(None)`) from one
/// on which 4.12 raises (`Err`).
pub fn try_raw_from_json(text: &str) -> Result<Option<Vec<Value>>, Raises> {
    let Some(entries) = backup_entries(text) else { return Ok(None) };
    let mut clean = Vec::new();
    for p in entries {
        if !matches!(p, Value::Object(_)) || py_strip(text_or_empty(&p, "material").ok_or(Raises)?).is_empty() {
            continue;
        }
        let material = py_strip(&py_str(get(&p, "material").ok_or(Raises)?).ok_or(Raises)?).to_string();
        let mut entry = vec![
            ("material".to_string(), Value::Str(material)),
            ("menge".to_string(), Value::Float(float_or_zero(&p, "menge").ok_or(Raises)?)),
            ("qualitaet".to_string(), get(&p, "qualitaet").cloned().unwrap_or(Value::Null)),
            ("ort".to_string(), Value::Str(py_strip(&place_text(&p).ok_or(Raises)?).to_string())),
        ];
        if truthy(get(&p, "gestohlen")) {
            entry.push(("gestohlen".to_string(), Value::Bool(true)));
        }
        clean.push(Value::Object(entry));
    }
    Ok(Some(clean))
}

/// `str(entry.get('ort') or '')`; `None` for a list or an object.
fn place_text(entry: &Value) -> Option<String> {
    match get(entry, "ort") {
        v if !truthy(v) => Some(String::new()),
        Some(v) => py_str(v),
        None => Some(String::new()),
    }
}

/// `str(entry.get('ware') or '').strip()` is not empty.
fn has_goods_text(entry: &Value) -> bool {
    match get(entry, "ware") {
        v if !truthy(v) => false,
        Some(v) => py_str(v).is_none_or(|s| !py_strip(&s).is_empty()),
        None => false,
    }
}

/// A goods backup read back, cleaned; nothing is saved
/// (`trade_cargo.from_json`). A list in which no entry has goods is the
/// other stock's backup and is rejected.
pub fn trade_from_json(text: &str) -> Option<Vec<Value>> {
    try_trade_from_json(text).ok().flatten()
}

/// [`trade_from_json`] telling „does not fit“ from „raises“, like
/// [`try_raw_from_json`].
pub fn try_trade_from_json(text: &str) -> Result<Option<Vec<Value>>, Raises> {
    let Some(entries) = backup_entries(text) else { return Ok(None) };
    if !entries.is_empty() && !entries.iter().any(|p| matches!(p, Value::Object(_)) && has_goods_text(p)) {
        return Ok(None);
    }
    let mut clean = Vec::new();
    for p in &entries {
        if !matches!(p, Value::Object(_)) || !has_goods_text(p) {
            continue;
        }
        let Some(amount) = float_or_zero(p, "menge") else { continue };
        if amount <= 0.0 {
            continue;
        }
        let goods = py_str(get(p, "ware").ok_or(Raises)?).ok_or(Raises)?;
        let mut entry = Value::Object(vec![
            ("ware".to_string(), Value::Str(py_strip(&goods).to_string())),
            ("menge".to_string(), Value::Float(amount)),
            ("ort".to_string(), Value::Str(py_strip(&place_text(p).ok_or(Raises)?).to_string())),
            ("gestohlen".to_string(), Value::Bool(truthy(get(p, "gestohlen")))),
        ]);
        if truthy(get(p, "stueck")) {
            set(&mut entry, "stueck", Value::Bool(true));
        }
        if let Some(uex @ Value::Int(_)) = get(p, "uex") {
            set(&mut entry, "uex", uex.clone());
        }
        set_cost(&mut entry, cost_of(p));
        clean.push(entry);
    }
    Ok(Some(clean))
}

/// What a backup file holds.
#[derive(Clone, Debug, PartialEq)]
pub enum Backup {
    Raw(Vec<Value>),
    Trade(Vec<Value>),
}

/// Materials or goods backup, told by the field of its entries (`ware`
/// or `material`); a file without entries is not taken
/// (`my_stock_page.detect_backup`).
pub fn detect_backup(text: &str) -> Option<Backup> {
    try_detect_backup(text).ok().flatten()
}

/// [`detect_backup`] telling „no backup“ from „raises“.
pub fn try_detect_backup(text: &str) -> Result<Option<Backup>, Raises> {
    let Ok(data) = jsonio::loads(text) else { return Ok(None) };
    let Some(Value::Array(entries)) = data.get("posten") else { return Ok(None) };
    // Like Python's `any`: stops at the first hit, raises at a non-text before it.
    let filled = |key: &str| -> Result<bool, Raises> {
        for e in entries.iter().filter(|e| matches!(e, Value::Object(_))) {
            if !py_strip(text_or_empty(e, key).ok_or(Raises)?).is_empty() {
                return Ok(true);
            }
        }
        Ok(false)
    };
    if filled("ware")? {
        return Ok(try_trade_from_json(text)?.filter(|f| !f.is_empty()).map(Backup::Trade));
    }
    if filled("material")? {
        return Ok(try_raw_from_json(text)?.filter(|f| !f.is_empty()).map(Backup::Raw));
    }
    Ok(None)
}

/// A goods backup read in: its goods replace the goods, the items here
/// stay, its items are added where no equal stack lies here; the new list
/// and (goods, items added) (`trade_cargo.restore`); `None` where an entry
/// here is no object (Python raises).
pub fn restore_trade(current: &[Value], found: &[Value]) -> Option<(Vec<Value>, usize, usize)> {
    if !current.iter().chain(found).all(|p| matches!(p, Value::Object(_))) {
        return None;
    }
    let items: Vec<Value> = current.iter().filter(|p| is_pieces(p)).cloned().collect();
    let goods: Vec<Value> = found.iter().filter(|p| !is_pieces(p)).cloned().collect();
    let added: Vec<Value> = found
        .iter()
        .filter(|p| is_pieces(p))
        .filter(|p| {
            let (ware, place) = (text_of(get(p, "ware")), text_of(get(p, "ort")));
            !items.iter().any(|q| same_trade_stack(ware, place, truthy(get(p, "gestohlen")), q))
        })
        .cloned()
        .collect();
    let counts = (goods.len(), added.len());
    let mut all = goods;
    all.extend(items);
    all.extend(added);
    Some((all, counts.0, counts.1))
}

// ------------------------------------------------------------ rows

/// Which file an entry lives in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Store {
    Raw,
    Trade,
}

/// One entry of either file in one form (`my_stock_page.entries`).
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub store: Store,
    pub index: usize,
    pub name: String,
    pub amount: f64,
    pub piece: bool,
    pub quality: Option<i64>,
    pub place: String,
    pub stolen: bool,
    pub cost: Option<f64>,
    pub item: bool,
    pub leftover: bool,
}

/// Both files as rows, materials first.
pub fn rows(raw: &[Value], trade: &[Value], data: &StockData) -> Vec<Row> {
    let mut out = Vec::new();
    for (index, entry) in raw.iter().enumerate() {
        let name = py_strip(text_of(get(entry, "material")));
        let quality = match get(entry, "qualitaet") {
            None | Some(Value::Null) => None,
            Some(Value::Str(s)) if s.is_empty() => None,
            Some(_) => Some(py_round_int(number_of(entry, "qualitaet"))),
        };
        out.push(Row {
            store: Store::Raw,
            index,
            name: if name.is_empty() { "?".into() } else { name.to_string() },
            amount: number_of(entry, "menge"),
            piece: data.is_piece(name),
            quality,
            place: py_strip(text_of(get(entry, "ort"))).to_string(),
            stolen: truthy(get(entry, "gestohlen")),
            cost: None,
            item: false,
            leftover: is_leftover(entry, data),
        });
    }
    for (index, entry) in trade.iter().enumerate() {
        let name = py_strip(text_of(get(entry, "ware")));
        let piece = is_pieces(entry);
        out.push(Row {
            store: Store::Trade,
            index,
            name: if name.is_empty() { "?".into() } else { name.to_string() },
            amount: number_of(entry, "menge"),
            piece,
            quality: None,
            place: py_strip(text_of(get(entry, "ort"))).to_string(),
            stolen: truthy(get(entry, "gestohlen")),
            cost: cost_of(entry),
            item: piece,
            leftover: false,
        });
    }
    out
}

// ------------------------------------------------------------ refinery

/// Why a refinery line was not taken, with its text key and values.
#[derive(Clone, Debug, PartialEq)]
pub enum LineError {
    TooShort,
    NoNumber,
    Unknown { name: String, similar: Vec<String> },
    Quality,
    Amount,
    WholePieces,
    PiecesForScu(String),
}

impl LineError {
    /// The reason as text; `key` may swap a text key for another one.
    pub fn text_with(&self, lang: Lang, key: impl Fn(&'static str) -> &'static str) -> String {
        let say = |k: &'static str, args: &[(&str, Arg)]| tf_in(lang, key(k), args);
        match self {
            LineError::TooShort => say("s_rf_zu_kurz", &[]),
            LineError::NoNumber => say("s_rf_keine_zahl", &[]),
            LineError::Unknown { name, similar } => {
                let mut text = say("s_rf_unbekannt", &[("0", Arg::Text(name.clone()))]);
                if !similar.is_empty() {
                    text.push(' ');
                    text.push_str(&say("s_rf_meintest", &[("0", Arg::Text(similar.join(" · ")))]));
                }
                text
            }
            LineError::Quality => say("s_rf_qualitaet", &[]),
            LineError::Amount => say("s_rf_menge", &[]),
            LineError::WholePieces => say("s_rf_stueck_ganz", &[]),
            LineError::PiecesForScu(name) => say("s_rf_stueck_scu", &[("0", Arg::Text(name.clone()))]),
        }
    }

    /// The reason in the words of the Python version.
    pub fn text_in(&self, lang: Lang) -> String {
        self.text_with(lang, |k| k)
    }
}

/// A refinery line read: material, amount in SCU (pieces for gems), quality.
#[derive(Clone, Debug, PartialEq)]
pub struct Yield {
    pub material: String,
    pub amount: f64,
    pub quality: i64,
}

/// The result of reading refinery lines.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RefineryLines {
    pub entries: Vec<Yield>,
    pub errors: Vec<(String, LineError)>,
    /// Lines whose unit looks mixed up: line, text key, amount.
    pub hints: Vec<(String, &'static str, f64)>,
}

/// A piece word at the end (`Stk`, `Stück`, `pcs`, `pc`, optional dot) after
/// a digit or space; returns where it starts.
fn piece_word(body: &str) -> Option<usize> {
    let chars: Vec<(usize, char)> = body.char_indices().collect();
    for (pos, &(start, _)) in chars.iter().enumerate() {
        if pos == 0 {
            continue;
        }
        let before = chars[pos - 1].1;
        if !(decimal_digit(before).is_some() || crate::text::py_isspace(before)) {
            continue;
        }
        let rest = body[start..].to_lowercase();
        let rest = rest.strip_suffix('.').unwrap_or(&rest);
        if matches!(rest, "stk" | "stück" | "pcs" | "pc") {
            return Some(start);
        }
    }
    None
}

/// Python's `str.splitlines()`.
fn split_lines(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut chars = text.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        if matches!(
            c,
            '\n' | '\r' | '\u{b}' | '\u{c}' | '\u{1c}' | '\u{1d}' | '\u{1e}' | '\u{85}' | '\u{2028}' | '\u{2029}'
        ) {
            out.push(&text[start..i]);
            let mut end = i + c.len_utf8();
            if c == '\r' && chars.peek().is_some_and(|(_, n)| *n == '\n') {
                chars.next();
                end += 1;
            }
            start = end;
        }
    }
    if start < text.len() {
        out.push(&text[start..]);
    }
    out
}

/// Read a refinery yield from typed text, one `Material Quality Amount` per
/// line (`materials.refinery_lines`). `cscu` is the default unit.
pub fn refinery_lines(text: &str, cscu: bool, data: &StockData) -> RefineryLines {
    let mut out = RefineryLines::default();
    for raw in split_lines(text) {
        let line = py_strip(raw);
        if line.is_empty() {
            continue;
        }
        let mut body = line.replace('\t', " ");
        let word = piece_word(&body);
        if let Some(start) = word {
            body = body[..start].trim_end_matches(crate::text::py_isspace).to_string();
        }
        let mut parts = py_split(&body);
        let mut line_cscu = cscu;
        let mut unit_word = false;
        if word.is_none()
            && let Some(last) = parts.last()
        {
            let low = last.to_lowercase();
            if low == "cscu" || low == "scu" {
                line_cscu = low == "cscu";
                parts.pop();
                unit_word = true;
            }
        }
        if parts.len() < 3 {
            out.errors.push((line.to_string(), LineError::TooShort));
            continue;
        }
        let name = parts[..parts.len() - 2].join(" ");
        let quality = py_float(&normalize_separators(parts[parts.len() - 2], false));
        let value = py_float(&normalize_separators(parts[parts.len() - 1], false));
        let (Some(quality), Some(value)) = (quality.filter(|q| q.is_finite()), value) else {
            out.errors.push((line.to_string(), LineError::NoNumber));
            continue;
        };
        let quality = quality.trunc() as i64;
        let Some(real) = data.storage_name(&name) else {
            let similar = data.similar_materials(&name, 2);
            out.errors.push((line.to_string(), LineError::Unknown { name, similar }));
            continue;
        };
        if !(0..=1000).contains(&quality) {
            out.errors.push((line.to_string(), LineError::Quality));
            continue;
        }
        if value <= 0.0 {
            out.errors.push((line.to_string(), LineError::Amount));
            continue;
        }
        if word.is_some() && (value - value.round_ties_even()).abs() > 1e-9 {
            out.errors.push((line.to_string(), LineError::WholePieces));
            continue;
        }
        if data.is_piece(&real) {
            out.entries.push(Yield { material: real, amount: value, quality });
            if unit_word {
                out.hints.push((line.to_string(), "s_lg_hinweis_stueck_einheit", value));
            }
            continue;
        }
        if word.is_some() {
            out.errors.push((line.to_string(), LineError::PiecesForScu(real)));
            continue;
        }
        let amount = py_round(value * if line_cscu { CSCU } else { 1.0 }, 4);
        out.entries.push(Yield { material: real, amount, quality });
        if let Some(hint) = unit_hint(value, line_cscu) {
            out.hints.push((line.to_string(), hint, amount));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stock_file_that_cannot_be_read_is_no_empty_stock_for_the_sync() {
        let dir = std::env::temp_dir().join(format!("versekit-lager-geprueft-{}", crate::testdir::run_tag()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("rohstoffe.json").to_string_lossy().replace('\\', "/");
        // No file: an empty stock, and that is right.
        assert_eq!(load_checked(&path, "test"), Some(Vec::new()));
        // A good file gives its rows.
        let row = Value::Object(vec![("material".into(), Value::str("Iron")), ("menge".into(), Value::Float(3.0))]);
        assert!(save(&path, std::slice::from_ref(&row), "test"));
        assert_eq!(load_checked(&path, "test"), Some(vec![row]));
        // A damaged file, another format and a missing list: nothing, where `load` would give an empty list.
        for broken in ["{kaputt", "{\"format\": 2, \"posten\": []}", "{\"format\": 1}", "[]"] {
            std::fs::write(&path, broken).unwrap();
            assert_eq!(load_checked(&path, "test"), None, "{broken}");
            assert!(load(&path, "test").is_empty(), "{broken}");
        }
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn numbers_like_python() {
        assert_eq!(parse_number("12,5"), Some(12.5));
        assert_eq!(parse_number("1.234,56"), Some(1234.56));
        assert_eq!(parse_number("1,234,567"), Some(1234567.0));
        assert_eq!(parse_number("1,500"), Some(1.5));
        assert_eq!(parse_number("−3"), Some(-3.0));
        assert_eq!(parse_number("abc"), None);
        assert_eq!(normalize_separators("17,200", true), "17200");
    }

    #[test]
    fn calculate_three_ways() {
        assert_eq!(calculate("4,5", 1.04), Some(4.5));
        assert_eq!(calculate("+3", 1.0), Some(4.0));
        assert_eq!(calculate("−0,5", 1.0), Some(0.5));
        assert_eq!(calculate("1.5+3", 0.0), Some(4.5));
        assert_eq!(calculate("10 - 4", 0.0), Some(6.0));
        assert_eq!(calculate("", 0.0), None);
        assert_eq!(calculate("1+", 0.0), None);
        assert!(is_calculation("1,04+3"));
        assert!(!is_calculation("4,5"));
    }

    #[test]
    fn amount_text_like_the_game() {
        assert_eq!(amount_text(3.0, false, ','), "3");
        assert_eq!(amount_text(0.02, false, ','), "0,020");
        assert_eq!(amount_text(3.96, false, '.'), "3.960");
        assert_eq!(amount_text(75.0, true, ','), "75");
        assert_eq!(amount_text(2.5, true, ','), "2,5");
    }

    #[test]
    fn trade_stacks_merge_and_mix_the_price() {
        let mut entries = Vec::new();
        assert_eq!(trade_add(&mut entries, " Gold ", "10", "Levski", false, "100"), Ok(()));
        assert_eq!(trade_add(&mut entries, "Gold", "+10", "Levski", false, "200"), Ok(()));
        assert_eq!(entries.len(), 1);
        assert_eq!(number_of(&entries[0], "menge"), 20.0);
        assert_eq!(cost_of(&entries[0]), Some(150.0));
        assert_eq!(trade_add(&mut entries, "Gold", "5", "Levski", true, ""), Ok(()));
        assert_eq!(entries.len(), 2);
        assert_eq!(trade_add(&mut entries, "Gold", "0", "", false, ""), Err(Refused::Amount));
        assert_eq!(trade_add(&mut entries, "Gold", "1", "", false, "-1"), Err(Refused::Price));
        assert_eq!(trade_change(&mut entries, 9, "Gold", "1", "", false, Cost::Keep), Err(Refused::Gone));
    }

    #[test]
    fn items_count_whole_pieces() {
        let mut entries = Vec::new();
        assert_eq!(trade_add_item(&mut entries, "FR-66", "2,5", "Levski", false, Some(7), None), Ok(()));
        assert_eq!(number_of(&entries[0], "menge"), 2.0);
        assert_eq!(trade_add_item(&mut entries, "FR-66", "0,5", "Levski", false, None, None), Err(Refused::Amount));
        assert_eq!(trade_add_item(&mut entries, "FR-66", "+1", "Levski", false, None, Some(0)), Ok(()));
        assert_eq!(number_of(&entries[0], "menge"), 3.0);
        assert!(entries[0].get("uex").is_none());
        assert_eq!(clear_items(&mut entries), 1);
    }

    #[test]
    fn an_item_booked_with_its_price_keeps_the_price_per_piece() {
        let mut entries = Vec::new();
        assert_eq!(trade_add_item_with_cost(&mut entries, " FR-66 ", "2", "Levski", false, Some(7), 100.0), Ok(()));
        assert_eq!(entries.len(), 1);
        assert_eq!(number_of(&entries[0], "menge"), 2.0);
        assert_eq!(cost_of(&entries[0]), Some(100.0));
        assert!(is_pieces(&entries[0]));
        assert_eq!(entries[0].get("uex"), Some(&Value::int(7)));
        assert_eq!(entries[0].get("ware"), Some(&Value::str("FR-66")));
        // The same stack: the price becomes the average per piece.
        assert_eq!(trade_add_item_with_cost(&mut entries, "FR-66", "3", "Levski", false, Some(7), 200.0), Ok(()));
        assert_eq!(entries.len(), 1);
        assert_eq!(number_of(&entries[0], "menge"), 5.0);
        assert_eq!(cost_of(&entries[0]), Some(160.0));
        // Another place is another stack with its own price.
        assert_eq!(trade_add_item_with_cost(&mut entries, "FR-66", "1", "Area 18", false, None, 90.0), Ok(()));
        assert_eq!(entries.len(), 2);
        assert_eq!(cost_of(&entries[1]), Some(90.0));
        assert_eq!(cost_of(&entries[0]), Some(160.0));
    }

    #[test]
    fn a_fraction_of_a_total_is_kept_with_four_decimals() {
        let mut entries = Vec::new();
        let third = 520.0 / 3.0;
        assert_eq!(trade_add_item_with_cost(&mut entries, "TS-2", "3", "Levski", false, None, third), Ok(()));
        assert_eq!(cost_of(&entries[0]), Some(173.3333));
        assert_eq!(trade_add_item_with_cost(&mut entries, "TS-2", "3", "Levski", false, None, third), Ok(()));
        assert_eq!(number_of(&entries[0], "menge"), 6.0);
        assert_eq!(cost_of(&entries[0]), Some(173.3333));
    }

    #[test]
    fn an_item_stack_without_a_price_does_not_make_one_up() {
        let mut entries = Vec::new();
        assert_eq!(trade_add_item(&mut entries, "FR-66", "2", "Levski", false, None, None), Ok(()));
        assert_eq!(cost_of(&entries[0]), None, "the old function books no price");
        assert!(entries[0].get("einkauf").is_none());
        assert_eq!(trade_add_item_with_cost(&mut entries, "FR-66", "1", "Levski", false, None, 100.0), Ok(()));
        assert_eq!(number_of(&entries[0], "menge"), 3.0);
        assert_eq!(cost_of(&entries[0]), None, "the average of two pieces without a price and one with is unknown");
    }

    #[test]
    fn a_refused_booking_with_a_price_changes_nothing() {
        let mut entries = Vec::new();
        assert_eq!(trade_add_item_with_cost(&mut entries, "FR-66", "2", "Levski", false, None, 100.0), Ok(()));
        let kept = entries.clone();
        for cost in [0.0, -5.0, f64::NAN, f64::INFINITY] {
            assert_eq!(
                trade_add_item_with_cost(&mut entries, "FR-66", "1", "Levski", false, None, cost),
                Err(Refused::Price)
            );
        }
        assert_eq!(
            trade_add_item_with_cost(&mut entries, "FR-66", "0", "Levski", false, None, 10.0),
            Err(Refused::Amount)
        );
        assert_eq!(trade_add_item_with_cost(&mut entries, "  ", "1", "Levski", false, None, 10.0), Err(Refused::Goods));
        assert_eq!(entries, kept);
    }

    #[test]
    fn raw_change_keeps_the_stolen_mark() {
        let data = StockData::default();
        let mut entries = Vec::new();
        raw_add(&mut entries, &data, " Iron ", 2.5, Some(500), " Levski ");
        set(&mut entries[0], "gestohlen", Value::Bool(true));
        assert!(raw_change(&mut entries, 0, "Iron", 1.0, Some(400), ""));
        assert_eq!(entries[0].get("gestohlen"), Some(&Value::Bool(true)));
        assert!(!raw_change(&mut entries, 3, "Iron", 1.0, None, ""));
    }

    #[test]
    fn refinery_lines_read_from_the_back() {
        let data = StockData::default();
        let read = refinery_lines("\nTitanium 295 188\nzu kurz\n", true, &data);
        assert_eq!(read.errors.len(), 2);
        assert!(matches!(read.errors[0].1, LineError::Unknown { .. }));
        assert_eq!(read.errors[1].1, LineError::TooShort);
        assert_eq!(piece_word("Hadanite 500 5Stk."), Some(14));
        assert_eq!(piece_word("Hadanite 500 5"), None);
    }
}
