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

//! Reading an export file into hangar entries, as `fleet.read` does: the
//! format is told by the content, not the file name.
//!
//! | file | read by |
//! |---|---|
//! | JSON of the Hangar Extension (`manufacturer` is an object) | `extension_entry` |
//! | JSON of the Hangar XPLORer (flat, `entity_type`) | `xplorer_entry` |
//! | Fleetyards, Fleetview (lists), StarJump (object) | `list_entry` |
//! | ship list `hangar-schiffe` 1 | `hangar::from_hangar_ships_with` |
//! | CSV of the Hangar Extension (`Pledge ID`, `Content Type`) | `from_extension_csv` |
//! | CSV of the Hangar XPLORer | `from_csv` |
//!
//! ⚠ Where Python raises on a value of an unexpected type, reading fails
//! the same way (`ReadFail::Error`); the whole file is then not taken.

use std::collections::HashSet;
use std::sync::OnceLock;

use regex::Regex;

use crate::errors;
use crate::hangar::{
    self, LIST, Rejected, ShipNames, View, catalog_name, extension_insurance, get, included_ships, iter_values, object,
    or_text, slim_value, split_maker, strip_owned, truthy,
};
use crate::jsonio::{self, Map, Value};
use crate::text::{py_isspace, py_strip};

/// Why a file gave no entries.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReadFail {
    /// A file read on purpose not (other version, transfer file).
    Rejected(Rejected),
    /// Not readable or not in a known form; the text Python would give.
    Error(String),
}

/// `fleet.read`: an export file → entries.
pub fn read_file(path: &str, names: &ShipNames) -> Result<Vec<Map>, ReadFail> {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(e) => {
            errors::record("fleet.read", &e);
            return Err(ReadFail::Error(e.to_string()));
        }
    };
    let text_ = match String::from_utf8(bytes) {
        Ok(t) => t,
        Err(e) => {
            errors::record("fleet.read", &e);
            return Err(ReadFail::Error(e.to_string()));
        }
    };
    read_text(&text_, names)
}

/// The content of an export file (still with BOM and line ends as stored).
pub fn read_text(raw: &str, names: &ShipNames) -> Result<Vec<Map>, ReadFail> {
    let raw = raw.strip_prefix('\u{feff}').unwrap_or(raw);
    // Python reads text with universal newlines.
    let text_ = raw.replace("\r\n", "\n").replace('\r', "\n");
    let head = text_.trim_start_matches(py_isspace).chars().next();
    let parsed = match head {
        Some('[' | '{') => from_json(&text_, names),
        _ => from_csv(&text_, names),
    };
    parsed.inspect_err(|fail| {
        if let ReadFail::Error(message) = fail {
            errors::record("fleet.read.parse", message);
        }
    })
}

fn err(message: impl Into<String>) -> ReadFail {
    ReadFail::Error(message.into())
}

/// `(value or '').strip()`.
fn stripped(m: &Map, key: &str) -> Result<String, ReadFail> {
    or_text(get(m, key)).map(strip_owned).map_err(err)
}

// ------------------------------------------------------------ JSON

fn from_json(text_: &str, names: &ShipNames) -> Result<Vec<Map>, ReadFail> {
    let raw = jsonio::loads(text_).map_err(|e| err(e.to_string()))?;
    if let Value::Object(map) = &raw {
        match get(map, "schema") {
            Some(Value::Str(s)) if s == hangar::HANGAR_SHIPS_SCHEMA => {
                return hangar::from_hangar_ships_with(map, names).map_err(ReadFail::Rejected);
            }
            Some(Value::Str(s)) if s == hangar::HANGAR_TRANSFER_SCHEMA => {
                return Err(ReadFail::Rejected(Rejected::Transfer));
            }
            _ => {}
        }
        if is_starjump(map) {
            return from_starjump(map, names);
        }
    }
    let Value::Array(items) = &raw else { return Ok(Vec::new()) };
    let mut codes_by_matrix: Vec<(String, HashSet<String>)> = Vec::new();
    for entry in items.iter().filter_map(Value::as_object) {
        if !matches!(get(entry, "manufacturer"), Some(Value::Object(_))) {
            continue;
        }
        let matrix = slim_value(get(entry, "matrix")).map_err(err)?;
        if matrix.is_empty() {
            continue;
        }
        let code = stripped(entry, "code")?;
        match codes_by_matrix.iter_mut().find(|(m, _)| *m == matrix) {
            Some((_, codes)) => {
                codes.insert(code);
            }
            None => codes_by_matrix.push((matrix, HashSet::from([code]))),
        }
    }
    let shared: HashSet<&String> = codes_by_matrix.iter().filter(|(_, c)| c.len() > 1).map(|(m, _)| m).collect();
    let mut result = Vec::new();
    let mut ext_raw: Vec<&Map> = Vec::new();
    let mut ext_ships: Vec<Option<Map>> = Vec::new();
    for entry in items.iter().filter_map(Value::as_object) {
        if matches!(get(entry, "manufacturer"), Some(Value::Object(_))) {
            let name_first = shared.contains(&slim_value(get(entry, "matrix")).map_err(err)?);
            let ship = extension_entry(entry, name_first)?;
            if let Some(ship) = &ship {
                result.push(ship.clone());
            }
            ext_raw.push(entry);
            ext_ships.push(ship);
            continue;
        }
        let has = |k: &str| get(entry, k).is_some();
        if !["pledge_id", "ship_code", "entity_type", "ship_name"].iter().any(|k| has(k)) {
            let ship = if ["shipCode", "manufacturerCode", "manufacturerName", "slug"].iter().any(|k| has(k)) {
                Some(from_fleetyards(entry, names)?)
            } else if has("shipname") || has("type") {
                Some(from_fleetview(entry, names)?)
            } else {
                None
            };
            if let Some(ship) = ship {
                result.extend(ship);
                continue;
            }
        }
        if let Some(ship) = xplorer_entry(entry)? {
            result.push(ship);
        }
    }
    let mut views = Vec::new();
    for (entry, ship) in ext_raw.iter().zip(&ext_ships) {
        // Only for the ships that were read; Python iterates nothing else.
        let included = if ship.is_some() { iter_values(get(entry, "includedShips")).map_err(err)? } else { Vec::new() };
        views.push(View {
            matrix: or_text(get(entry, "matrix")).map_err(err)?.to_string(),
            name: or_text(get(entry, "name")).map_err(err)?.to_string(),
            included,
        });
    }
    result.extend(included_ships(&views, &ext_ships, names));
    Ok(result)
}

/// `fleet._hangar_extension_entry`.
fn extension_entry(entry: &Map, name_first: bool) -> Result<Option<Map>, ReadFail> {
    let empty = Map::new();
    let maker = match get(entry, "manufacturer") {
        Some(Value::Object(m)) => m,
        _ => &empty,
    };
    let matrix = stripped(entry, "matrix")?;
    let label = stripped(entry, "name")?;
    let name = if name_first {
        if label.is_empty() { matrix } else { label }
    } else if matrix.is_empty() {
        label
    } else {
        matrix
    };
    if name.is_empty() {
        return Ok(None);
    }
    let (lti, months) = extension_insurance(entry);
    let maker_code = match get(maker, "code") {
        v if truthy(v) => v,
        _ => get(maker, "shortName"),
    };
    Ok(Some(object(vec![
        ("name", Value::str(name)),
        ("hersteller", Value::str(stripped(maker, "name")?)),
        ("kurz", Value::str(stripped(entry, "code")?)),
        ("hkurz", Value::str(strip_owned(or_text(maker_code).map_err(err)?))),
        ("lti", Value::Bool(lti)),
        ("warbond", Value::Bool(false)),
        ("versicherung", if months != 0 && !lti { Value::int(months) } else { Value::Null }),
        ("paket", Value::str(stripped(entry, "includedWith")?)),
        ("gekauft", Value::str("")),
        ("preis", Value::str("")),
    ])))
}

/// One flat entry of the Hangar XPLORer; `None` for equipment, paints, suits.
fn xplorer_entry(entry: &Map) -> Result<Option<Map>, ReadFail> {
    match get(entry, "entity_type") {
        None | Some(Value::Null) => {}
        Some(Value::Str(s)) if s.is_empty() || s == "ship" => {}
        _ => return Ok(None),
    }
    // `name` before `ship_name`: `ship_name` is the base type.
    let name = match get(entry, "name") {
        v if truthy(v) => v,
        _ => get(entry, "ship_name"),
    };
    let name = strip_owned(or_text(name).map_err(err)?);
    if name.is_empty() {
        return Ok(None);
    }
    Ok(Some(object(vec![
        ("name", Value::str(name)),
        ("hersteller", Value::str(stripped(entry, "manufacturer_name")?)),
        ("kurz", Value::str(stripped(entry, "ship_code")?)),
        ("hkurz", Value::str(stripped(entry, "manufacturer_code")?)),
        ("lti", Value::Bool(truthy(get(entry, "lti")))),
        ("warbond", Value::Bool(truthy(get(entry, "warbond")))),
        ("paket", Value::str(stripped(entry, "pledge_name")?)),
        ("gekauft", Value::str(stripped(entry, "pledge_date")?)),
        ("preis", Value::str(stripped(entry, "pledge_cost")?)),
    ])))
}

// ------------------------------------------------------------ ship lists

/// `fleet._list_entry`: a ship of another tool's list; it only adds, never
/// removes (`herkunft` = `liste`).
fn list_entry(
    name: Option<&Value>,
    source: &str,
    maker: Option<&Value>,
    maker_code: Option<&Value>,
    wanted: bool,
    names: &ShipNames,
) -> Result<Option<Map>, ReadFail> {
    let name = strip_owned(or_text(name).map_err(err)?);
    if name.is_empty() {
        return Ok(None);
    }
    let (name, maker, maker_code) = if truthy(maker_code) {
        (name, or_text(maker).map_err(err)?.to_string(), or_text(maker_code).map_err(err)?.to_string())
    } else {
        catalog_name(&name, names)
    };
    Ok(Some(object(vec![
        ("name", Value::str(name)),
        ("hersteller", Value::str(py_strip(&maker))),
        ("kurz", Value::str("")),
        ("hkurz", Value::str(py_strip(&maker_code))),
        ("lti", Value::Bool(false)),
        ("warbond", Value::Bool(false)),
        ("paket", Value::str("")),
        ("gekauft", Value::str("")),
        ("preis", Value::str("")),
        ("herkunft", Value::str(LIST)),
        ("quelle", Value::str(source)),
        ("wunsch", Value::Bool(wanted)),
    ])))
}

fn from_fleetview(entry: &Map, names: &ShipNames) -> Result<Option<Map>, ReadFail> {
    match get(entry, "type") {
        None | Some(Value::Null) => {}
        Some(Value::Str(s)) if s.is_empty() || s == "ship" => {}
        _ => return Ok(None),
    }
    list_entry(get(entry, "name"), "Fleetview", None, None, false, names)
}

fn from_fleetyards(entry: &Map, names: &ShipNames) -> Result<Option<Map>, ReadFail> {
    let name = or_text(get(entry, "name")).map_err(err)?;
    if py_strip(name).is_empty() {
        return Ok(None);
    }
    list_entry(
        Some(&Value::str(name)),
        "Fleetyards",
        get(entry, "manufacturerName"),
        get(entry, "manufacturerCode"),
        matches!(get(entry, "wanted"), Some(Value::Bool(true))),
        names,
    )
}

fn is_starjump(raw: &Map) -> bool {
    matches!(get(raw, "type"), Some(Value::Str(s)) if s.to_lowercase() == "starjumpfleetviewer")
        || matches!(get(raw, "canvasItems"), Some(Value::Array(_)))
}

/// The FleetViewer of StarJump: an object, the ships under `canvasItems`.
fn from_starjump(raw: &Map, names: &ShipNames) -> Result<Vec<Map>, ReadFail> {
    let mut result = Vec::new();
    for item in iter_values(get(raw, "canvasItems")).map_err(err)? {
        let Value::Object(item) = item else { continue };
        if or_text(get(&item, "itemType")).map_err(err)?.to_lowercase() != "ship" {
            continue;
        }
        let mut name = stripped(&item, "defaultText")?;
        if name.is_empty() {
            name = strip_owned(&or_text(get(&item, "shipSlug")).map_err(err)?.replace('-', " "));
        }
        if let Some(ship) = list_entry(Some(&Value::str(name)), "StarJump", None, None, false, names)? {
            result.push(ship);
        }
    }
    Ok(result)
}

// ------------------------------------------------------------ CSV

/// Python's `csv.field_size_limit()`.
const FIELD_LIMIT: usize = 131_072;

#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    StartRecord,
    StartField,
    InField,
    InQuotedField,
    QuoteInQuotedField,
    EatCrnl,
}

/// The record being read: its fields and the field so far.
#[derive(Default)]
struct Record {
    fields: Vec<String>,
    field: String,
    len: usize,
}

impl Record {
    fn add(&mut self, c: char) -> Result<(), String> {
        if self.len >= FIELD_LIMIT {
            return Err(format!("field larger than field limit ({FIELD_LIMIT})"));
        }
        self.field.push(c);
        self.len += 1;
        Ok(())
    }

    fn save(&mut self) {
        self.fields.push(std::mem::take(&mut self.field));
        self.len = 0;
    }
}

/// `csv.reader(lines, skipinitialspace=True)` with the `excel` dialect:
/// one list of fields per record; a blank line gives an empty record.
/// `None` stands for the end of a line (`EOL` in `_csv.c`).
fn csv_records(text_: &str) -> Result<Vec<Vec<String>>, String> {
    let mut records = Vec::new();
    let mut lines = text_.split_inclusive('\n').peekable();
    while lines.peek().is_some() {
        let mut r = Record::default();
        let mut state = State::StartRecord;
        loop {
            let Some(line) = lines.next() else {
                if r.len != 0 || state == State::InQuotedField {
                    r.save();
                }
                break;
            };
            for c in line.chars().map(Some).chain([None]) {
                let end = matches!(c, None | Some('\n' | '\r'));
                let after_end = if c.is_none() { State::StartRecord } else { State::EatCrnl };
                if state == State::StartRecord {
                    match c {
                        None => continue,
                        Some('\n' | '\r') => {
                            state = State::EatCrnl;
                            continue;
                        }
                        Some(_) => state = State::StartField,
                    }
                }
                state = match (state, c) {
                    (State::InQuotedField, None) => State::InQuotedField,
                    (State::InQuotedField, Some('"')) => State::QuoteInQuotedField,
                    (State::InQuotedField, Some(c)) => {
                        r.add(c)?;
                        State::InQuotedField
                    }
                    (State::QuoteInQuotedField, Some('"')) => {
                        r.add('"')?;
                        State::InQuotedField
                    }
                    (State::EatCrnl, None) => State::StartRecord,
                    (State::EatCrnl, Some('\n' | '\r')) => State::EatCrnl,
                    (State::EatCrnl, Some(_)) => return Err("new-line character seen in unquoted field".into()),
                    (_, _) if end => {
                        r.save();
                        after_end
                    }
                    (State::StartField, Some('"')) => State::InQuotedField,
                    (State::StartField, Some(' ')) => State::StartField,
                    (_, Some(',')) => {
                        r.save();
                        State::StartField
                    }
                    (_, Some(c)) => {
                        r.add(c)?;
                        State::InField
                    }
                    (_, None) => State::StartRecord,
                };
            }
            if state == State::StartRecord {
                break;
            }
        }
        records.push(r.fields);
    }
    Ok(records)
}

/// One row of `csv.DictReader`: header name → value (`None` past the end
/// of a short row); a later column of the same name wins.
type Row = Vec<(String, Option<String>)>;

fn row_get<'a>(row: &'a Row, key: &str) -> &'a str {
    row.iter().rev().find(|(k, _)| k == key).and_then(|(_, v)| v.as_deref()).unwrap_or("")
}

/// `fleet._from_csv`: the XPLORer CSV, or the extension CSV by its header.
fn from_csv(text_: &str, _names: &ShipNames) -> Result<Vec<Map>, ReadFail> {
    let mut records = csv_records(text_).map_err(err)?.into_iter();
    let header = records.next().unwrap_or_default();
    let rows: Vec<Row> = records
        .filter(|r| !r.is_empty())
        .map(|r| header.iter().enumerate().map(|(i, h)| (h.clone(), r.get(i).cloned())).collect())
        .collect();
    let fields: Vec<&str> = header.iter().map(|f| py_strip(f)).collect();
    if fields.contains(&"Pledge ID") && fields.contains(&"Content Type") {
        return Ok(from_extension_csv(&rows));
    }
    let mut result = Vec::new();
    for row in &rows {
        let name = py_strip(row_get(row, "Ship"));
        // The export writes `undefined` for unknown pieces: not a ship.
        if name.is_empty() || name == "undefined" {
            continue;
        }
        let flag = |key: &str| Value::Bool(py_strip(row_get(row, key)).to_lowercase() == "true");
        result.push(object(vec![
            ("name", Value::str(name)),
            ("hersteller", Value::str(py_strip(row_get(row, "Manufacturer")))),
            ("kurz", Value::str("")),
            ("hkurz", Value::str("")),
            ("lti", flag("Lti")),
            ("warbond", flag("Warbond")),
            ("paket", Value::str(py_strip(row_get(row, "Pledge")))),
            ("gekauft", Value::str(py_strip(row_get(row, "Date")))),
            ("preis", Value::str(py_strip(row_get(row, "Cost")))),
        ]));
    }
    Ok(result)
}

/// `fleet._yes_no`.
fn yes_no(value: &str) -> Option<bool> {
    match py_strip(value).to_lowercase().as_str() {
        "yes" | "true" | "1" => Some(true),
        "no" | "false" | "0" => Some(false),
        _ => None,
    }
}

static MONTHS: OnceLock<Option<Regex>> = OnceLock::new();

/// `Lifetime Insurance` → (true, 0) · `120 Month Insurance` → (false, 120).
fn insurance(content: &str) -> (bool, i64) {
    let t = py_strip(content);
    if t.to_lowercase().contains("lifetime") {
        return (true, 0);
    }
    let re = MONTHS.get_or_init(|| Regex::new(&format!(r"(?i)^{s}*(\d+){s}*month", s = crate::text::PY_SPACE)).ok());
    let months = re
        .as_ref()
        .and_then(|re| re.captures(t))
        .map_or(0, |c| hangar::py_int(Some(&Value::str(&c[1]))).unwrap_or(i64::MAX));
    (false, months)
}

struct Pledge {
    id: String,
    ships: Vec<String>,
    included: Vec<String>,
    lti: bool,
    months: i64,
    name: String,
    date: String,
    cost: String,
    meltable: Option<bool>,
}

/// `fleet._from_extension_csv`: one row per content of a pledge, held
/// together by `Pledge ID`; the insurance row counts for every ship of it.
fn from_extension_csv(rows: &[Row]) -> Vec<Map> {
    let mut pledges: Vec<Pledge> = Vec::new();
    for row in rows {
        let id = py_strip(row_get(row, "Pledge ID")).to_string();
        let index = match pledges.iter().position(|p| p.id == id) {
            Some(i) => i,
            None => {
                pledges.push(Pledge {
                    id,
                    ships: Vec::new(),
                    included: Vec::new(),
                    lti: false,
                    months: 0,
                    name: py_strip(row_get(row, "Pledge Name")).to_string(),
                    date: py_strip(row_get(row, "Pledge Date")).to_string(),
                    cost: py_strip(row_get(row, "Pledge Cost")).to_string(),
                    meltable: yes_no(row_get(row, "Meltable")),
                });
                pledges.len() - 1
            }
        };
        let p = &mut pledges[index];
        if p.meltable.is_none() {
            p.meltable = yes_no(row_get(row, "Meltable"));
        }
        let kind = py_strip(row_get(row, "Content Type")).to_lowercase();
        let content = py_strip(row_get(row, "Pledge Content")).to_string();
        if kind == "ship" && !content.is_empty() {
            p.ships.push(content);
        } else if kind == "included ship" && !content.is_empty() {
            p.included.push(content);
        } else if kind == "insurance" {
            let (lti, months) = insurance(&content);
            p.lti = p.lti || lti;
            p.months = p.months.max(months);
        }
    }
    let mut result = Vec::new();
    for p in &pledges {
        let all = p.ships.iter().map(|s| (s, false)).chain(p.included.iter().map(|s| (s, true)));
        for (full, included) in all {
            let (maker, code, name) = split_maker(full);
            if name.is_empty() {
                continue;
            }
            // An included ship: with one ship in the pledge that is the carrier.
            let package = if included && p.ships.len() == 1 { split_maker(&p.ships[0]).2 } else { p.name.clone() };
            result.push(object(vec![
                ("name", Value::str(name)),
                ("hersteller", Value::str(maker)),
                ("kurz", Value::str("")),
                ("hkurz", Value::str(code)),
                ("lti", Value::Bool(p.lti)),
                ("warbond", Value::Bool(false)),
                ("paket", Value::str(package)),
                ("gekauft", Value::str(p.date.clone())),
                ("preis", Value::str(p.cost.clone())),
                ("versicherung", if p.months != 0 && !p.lti { Value::int(p.months) } else { Value::Null }),
                ("schmelzbar", p.meltable.map_or(Value::Null, Value::Bool)),
            ]));
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hangar::text;

    fn read(text_: &str) -> Result<Vec<Map>, ReadFail> {
        read_text(text_, &ShipNames::default())
    }

    fn dump(entries: &[Map]) -> String {
        jsonio::dumps(&Value::Array(entries.iter().cloned().map(Value::Object).collect()), None, false)
    }

    #[test]
    fn xplorer_json_takes_name_before_ship_name_and_only_ships() {
        let entries = read(
            r#"[{"name": "ATLS GEO", "ship_name": "ATLS", "ship_code": "ARGO_ATLS", "manufacturer_code": "ARGO",
                 "manufacturer_name": "Argo", "lti": 1, "pledge_name": "Standalone Ship", "entity_type": "ship"},
                {"name": "Bosco Rack", "entity_type": "component"}]"#,
        )
        .unwrap();
        assert_eq!(
            dump(&entries),
            r#"[{"name": "ATLS GEO", "hersteller": "Argo", "kurz": "ARGO_ATLS", "hkurz": "ARGO", "lti": true, "warbond": false, "paket": "Standalone Ship", "gekauft": "", "preis": ""}]"#
        );
    }

    #[test]
    fn extension_json_with_included_ships() {
        let entries = read(
            r#"[{"manufacturer": {"code": "ANVL", "name": "Anvil"}, "code": "ANVL_Carrack", "matrix": "Carrack",
                 "name": "carrack", "insurance": "120MI", "includedShips": ["C8 Pisces"]}]"#,
        )
        .unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(text(&entries[0], "name"), "Carrack");
        assert_eq!(get(&entries[0], "versicherung"), Some(&Value::int(120)));
        assert_eq!(text(&entries[1], "paket"), "Carrack");
    }

    #[test]
    fn a_number_where_text_belongs_fails_like_python() {
        assert!(matches!(read(r#"[{"manufacturer": {"name": 5}, "matrix": "X"}]"#), Err(ReadFail::Error(_))));
        assert!(matches!(read(r#"[{"name": 7, "ship_code": "A"}]"#), Err(ReadFail::Error(_))));
    }

    #[test]
    fn lists_of_other_tools() {
        let fleetyards = read(
            r#"[{"name": "A1 Spirit", "manufacturerName": "Crusader", "manufacturerCode": "CRUS", "wanted": true}]"#,
        )
        .unwrap();
        assert_eq!(text(&fleetyards[0], "quelle"), "Fleetyards");
        assert_eq!(get(&fleetyards[0], "wunsch"), Some(&Value::Bool(true)));
        let fleetview = read(r#"[{"name": "aurora mr", "shipname": "", "type": "ship"}]"#).unwrap();
        assert_eq!(text(&fleetview[0], "name"), "Aurora MR");
        let starjump = read(
            r#"{"type": "starjumpFleetviewer", "canvasItems": [{"itemType": "SHIP", "shipSlug": "zeus-mkii-mr"}]}"#,
        )
        .unwrap();
        assert_eq!(text(&starjump[0], "name"), "Zeus Mkii MR");
    }

    #[test]
    fn csv_quotes_spaces_and_line_ends() {
        let records = csv_records("a, \"b,c\",\"d\"\"e\"\n\nx\"y\"z,\"q\"r\r\n").unwrap();
        assert_eq!(records, vec![vec!["a", "b,c", "d\"e"], vec![], vec!["x\"y\"z", "qr"]]);
        assert!(csv_records("a\rb").is_err());
    }

    #[test]
    fn xplorer_csv_and_extension_csv() {
        let xplorer = read("Manufacturer, Ship, Lti, Warbond, Pledge, Date, Cost\nDrake, Cutlass Black, True, False, Standalone, May 1, $100\n, undefined,,,,,\n").unwrap();
        assert_eq!(xplorer.len(), 1);
        assert_eq!(get(&xplorer[0], "lti"), Some(&Value::Bool(true)));
        let extension = read(
            "Pledge ID,Pledge Name,Pledge Date,Pledge Cost,Meltable,Content Type,Pledge Content\n\
             1,Pack,2024,$10,Yes,Ship,Aegis Idris-P\n\
             1,Pack,2024,$10,,Included Ship,ARGO MPUV Personnel\n\
             1,Pack,2024,$10,,Insurance,120 Month Insurance\n",
        )
        .unwrap();
        assert_eq!(extension.len(), 2);
        assert_eq!(text(&extension[0], "hkurz"), "AEGS");
        assert_eq!(text(&extension[1], "paket"), "Idris-P");
        assert_eq!(get(&extension[1], "versicherung"), Some(&Value::int(120)));
        assert_eq!(get(&extension[0], "schmelzbar"), Some(&Value::Bool(true)));
    }

    #[test]
    fn rejected_files() {
        assert_eq!(read(r#"{"schema": "hangar-uebergabe"}"#), Err(ReadFail::Rejected(Rejected::Transfer)));
        assert_eq!(read(r#"{"schema": "hangar-schiffe", "version": 2}"#), Err(ReadFail::Rejected(Rejected::Version)));
        assert!(matches!(read("[1, 2"), Err(ReadFail::Error(_))));
        assert_eq!(read("  {\"a\": 1}").unwrap().len(), 0);
    }
}
