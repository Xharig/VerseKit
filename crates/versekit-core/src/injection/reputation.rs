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

//! Whom a contract credits reputation, of which kind, and where it plays:
//! the stored table of `reputation.py` (`auftragsruf.json`) and the lines
//! it gives for a contract text.
//!
//! Whom a contract credits comes from the player's `Data.p4k`
//! (`versekit_gamedata::contract_reputation`), with scmdb's contract file
//! (`versekit_gamedata::contract_info`) as fallback and cross-check; where it
//! plays comes from scmdb's file. Only the result is stored, bound to the
//! game version.

use crate::errors;
use crate::jsonio::{self, Map, Value};
use crate::paths::Paths;

pub const CACHE_FILE: &str = "auftragsruf.json";
/// From 2 with regions, from 3 with the terms of every contract (`angaben`); a table of format 1 counts as empty, one
/// of format 2 counts until the catalog is built anew.
pub const FORMAT: i64 = 3;
/// The oldest format that is still read.
const OLDEST_FORMAT: i64 = 2;

/// `reputation._key`: scmdb writes the key with `@` and in other case.
pub fn key(raw: &str) -> String {
    raw.trim_start_matches('@').to_lowercase()
}

/// The stored table (`{'version', 'auftraege', 'regionen', 'datenfehler', 'angaben', 'angaben_quelle'}`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Table {
    pub version: String,
    pub contracts: Map,
    pub regions: Map,
    pub fingerprint: String,
    /// The terms of every contract by title key (see [`super::terms`]).
    pub terms: Map,
    /// Where the terms come from: [`TERMS_FROM_GAME`] or [`TERMS_FROM_SCMDB`].
    pub terms_source: String,
}

/// The terms come from the player's game data.
pub const TERMS_FROM_GAME: &str = "spieldaten";
/// The terms come from scmdb's file.
pub const TERMS_FROM_SCMDB: &str = "scmdb";

/// `reputation.load`.
pub fn load(paths: &Paths) -> Table {
    let Ok(data) = jsonio::read_json(&paths.app_file(CACHE_FILE)) else { return Table::default() };
    let format = match data.get("format") {
        Some(Value::Int(text)) => text.parse().unwrap_or(0),
        _ => 0,
    };
    if !(OLDEST_FORMAT..=FORMAT).contains(&format) {
        return Table::default();
    }
    let Some(Value::Object(contracts)) = data.get("auftraege") else { return Table::default() };
    Table {
        version: data.get("version").and_then(Value::as_str).unwrap_or("").to_string(),
        contracts: contracts.clone(),
        regions: data.get("regionen").and_then(Value::as_object).cloned().unwrap_or_default(),
        fingerprint: data.get("datenfehler").and_then(Value::as_str).unwrap_or("").to_string(),
        terms: data.get("angaben").and_then(Value::as_object).cloned().unwrap_or_default(),
        terms_source: data.get("angaben_quelle").and_then(Value::as_str).unwrap_or("").to_string(),
    }
}

/// `reputation.save`: keys in the order 4.12 writes them.
pub fn save(paths: &Paths, table: &Table) -> bool {
    let data: Map = vec![
        ("format".into(), Value::int(FORMAT)),
        ("version".into(), Value::str(&table.version)),
        ("auftraege".into(), Value::Object(table.contracts.clone())),
        ("regionen".into(), Value::Object(table.regions.clone())),
        ("datenfehler".into(), Value::str(&table.fingerprint)),
        ("angaben".into(), Value::Object(table.terms.clone())),
        ("angaben_quelle".into(), Value::str(&table.terms_source)),
    ];
    let text = jsonio::dumps(&Value::Object(data), None, false);
    match jsonio::write_text_atomic(&paths.app_file(CACHE_FILE), &text, jsonio::Newline::Keep) {
        Ok(()) => true,
        Err(error) => {
            errors::record("reputation.save", &error);
            false
        }
    }
}

/// The rows of the contracts that share a title, joined: `contracts` holds, per contract, the key of its title and its
/// rows (`{wer, was, wieviel}`, maybe `datenfehler`). One row per party and kind in the order they first occur (a
/// contract that names the same party and kind twice keeps two rows); its amount is the one every contract gives, or
/// `[least, most]` when they differ. A row only stands where every contract of the title gives it: one that some
/// contracts give and others not tells nothing sure of the title. A title without rows is left out.
pub fn merge_parties(contracts: &[(String, Vec<Map>)]) -> Map {
    type Row = (String, String, usize, Vec<Value>, Option<Value>);
    // Per title: the contracts it has, and its rows.
    let mut out: Vec<(String, usize, Vec<Row>)> = Vec::new();
    for (title, rows) in contracts.iter().filter(|(title, _)| !title.is_empty()) {
        let at = match out.iter().position(|(known, _, _)| known == title) {
            Some(at) => at,
            None => {
                out.push((title.clone(), 0, Vec::new()));
                out.len() - 1
            }
        };
        out[at].1 += 1;
        let merged = &mut out[at].2;
        let mut seen: Vec<(String, String)> = Vec::new();
        for row in rows {
            let field = |name: &str| row.iter().find(|(k, _)| k == name).map(|(_, v)| v.clone());
            let text = |name: &str| field(name).and_then(|v| v.as_str().map(String::from)).unwrap_or_default();
            let (who, what) = (text("wer"), text("was"));
            let Some(amount) = field("wieviel") else { continue };
            let nth = seen.iter().filter(|(w, k)| *w == who && *k == what).count();
            seen.push((who.clone(), what.clone()));
            match merged.iter_mut().find(|(w, k, n, _, _)| *w == who && *k == what && *n == nth) {
                Some((_, _, _, amounts, error)) => {
                    amounts.push(amount);
                    if error.is_none() {
                        *error = field("datenfehler");
                    }
                }
                None => merged.push((who, what, nth, vec![amount], field("datenfehler"))),
            }
        }
    }
    out.into_iter()
        .filter_map(|(title, contracts, rows)| {
            let rows: Vec<Value> = rows
                .into_iter()
                .filter(|(_, _, _, amounts, _)| amounts.len() == contracts)
                .map(|(who, what, _, amounts, error)| {
                    let mut row: Map =
                        vec![("wer".into(), Value::Str(who)), ("was".into(), Value::Str(what)), ("wieviel".into(), span(&amounts))];
                    if let Some(error) = error {
                        row.push(("datenfehler".into(), error));
                    }
                    Value::Object(row)
                })
                .collect();
            (!rows.is_empty()).then_some((title, Value::Array(rows)))
        })
        .collect()
}

/// One amount when all are the same, else `[least, most]`.
fn span(amounts: &[Value]) -> Value {
    let as_number = |v: &Value| match v {
        Value::Int(text) => text.parse::<f64>().ok(),
        Value::Float(x) => Some(*x),
        _ => None,
    };
    if amounts.windows(2).all(|pair| pair[0] == pair[1]) {
        return amounts.first().cloned().unwrap_or(Value::Null);
    }
    let by_size = |a: &&Value, b: &&Value| {
        as_number(a).unwrap_or(f64::NAN).partial_cmp(&as_number(b).unwrap_or(f64::NAN)).unwrap_or(std::cmp::Ordering::Equal)
    };
    let least = amounts.iter().min_by(by_size).cloned().unwrap_or(Value::Null);
    let most = amounts.iter().max_by(by_size).cloned().unwrap_or(Value::Null);
    Value::Array(vec![least, most])
}

/// An amount as the line shows it, with its sign: `+50`, `-50`, or `+50–200` and `-50–200` for a span.
fn amount_text(value: Option<&Value>) -> String {
    let signed = |text: String| if text.starts_with('-') { text } else { format!("+{text}") };
    match value {
        Some(Value::Array(pair)) if pair.len() == 2 => {
            signed(format!("{}–{}", super::texts::py_str(&pair[0]), super::texts::py_str(&pair[1])))
        }
        Some(value) => signed(super::texts::py_str(value)),
        None => "None".into(),
    }
}

/// `reputation.same_build`: does `4.10.1-live.12660092` belong to `4.10.1`?
pub fn same_build(full: &str, game_version: &str) -> bool {
    !full.is_empty()
        && !game_version.is_empty()
        && (full == game_version || full.starts_with(&format!("{game_version}-")))
}

fn lookup<'a>(map: &'a Map, raw_key: &str) -> Option<&'a Value> {
    let wanted = key(raw_key);
    map.iter().find(|(k, _)| *k == wanted).map(|(_, v)| v)
}

/// `reputation.line`: `# Ruf: Headhunters +50 Standing`, or empty.
pub fn line(table: &Table, raw_key: &str, word: &str) -> String {
    let mut parts = Vec::new();
    if let Some(Value::Array(entries)) = lookup(&table.contracts, raw_key) {
        for entry in entries {
            let amount = amount_text(entry.get("wieviel"));
            let text =
                |name: &str| crate::text::py_strip(entry.get(name).and_then(Value::as_str).unwrap_or("")).to_string();
            let (who, what) = (text("wer"), text("was"));
            match (who.is_empty(), what.is_empty()) {
                (false, false) => parts.push(format!("{who} {amount} {what}")),
                (false, true) => parts.push(format!("{who} {amount}")),
                (true, false) => parts.push(format!("{amount} {what}")),
                (true, true) => {}
            }
        }
    }
    if parts.is_empty() { String::new() } else { format!("# {word}: {}", parts.join(", ")) }
}

/// `reputation.region_line`: `# Region: Pyro (Bloom)`, or empty.
pub fn region_line(table: &Table, raw_key: &str, word: &str) -> String {
    match lookup(&table.regions, raw_key).and_then(Value::as_str).filter(|t| !t.is_empty()) {
        Some(text) => format!("# {word}: {text}"),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(json: &str) -> Vec<Map> {
        match jsonio::loads(json).unwrap() {
            Value::Array(list) => list.into_iter().filter_map(|v| v.as_object().cloned()).collect(),
            _ => panic!("a list"),
        }
    }

    /// The variants of a title in any order give the same rows: one per party and kind, a span where they differ.
    #[test]
    fn the_variants_of_a_title_are_joined_whatever_their_order() {
        let a = rows(r#"[{"wer": "HH", "was": "Standing", "wieviel": 75}, {"wer": "HH", "was": "Affinity", "wieviel": 50}]"#);
        let b = rows(r#"[{"wer": "HH", "was": "Affinity", "wieviel": 200}, {"wer": "HH", "was": "Standing", "wieviel": 75}]"#);
        let c = rows(r#"[{"wer": "HH", "was": "Standing", "wieviel": 75}, {"wer": "HH", "was": "Affinity", "wieviel": 50}]"#);
        let one = merge_parties(&[("t".into(), a.clone()), ("t".into(), b.clone()), ("t".into(), c.clone()), ("e".into(), vec![])]);
        let other = merge_parties(&[("t".into(), c), ("t".into(), b), ("t".into(), a)]);
        let text = |map: &Map| jsonio::dumps(&Value::Object(map.clone()), None, true);
        assert_eq!(text(&one), text(&other));
        assert_eq!(
            jsonio::dumps(&Value::Object(one.clone()), None, false),
            r#"{"t": [{"wer": "HH", "was": "Standing", "wieviel": 75}, {"wer": "HH", "was": "Affinity", "wieviel": [50, 200]}]}"#
        );
        let table = Table { contracts: one, ..Table::default() };
        assert_eq!(line(&table, "T", "Ruf"), "# Ruf: HH +75 Standing, HH +50–200 Affinity");
    }

    /// A row that some contracts of a title give and others not stands for none of them; a title left without rows
    /// is left out.
    #[test]
    fn a_row_not_every_contract_gives_is_left_out() {
        let both = rows(r#"[{"wer": "HH", "was": "Standing", "wieviel": 75}, {"wer": "HH", "was": "Affinity", "wieviel": 50}]"#);
        let one = rows(r#"[{"wer": "HH", "was": "Standing", "wieviel": 100}]"#);
        let merged = merge_parties(&[
            ("t".into(), both.clone()),
            ("t".into(), one),
            ("n".into(), both),
            ("n".into(), vec![]),
        ]);
        assert_eq!(
            jsonio::dumps(&Value::Object(merged), None, false),
            r#"{"t": [{"wer": "HH", "was": "Standing", "wieviel": [75, 100]}]}"#
        );
    }

    /// A loss of reputation keeps its sign.
    #[test]
    fn a_loss_shows_with_its_sign() {
        let table = Table {
            contracts: merge_parties(&[(
                "t".into(),
                rows(r#"[{"wer": "CFP", "was": "Standing", "wieviel": -50}, {"wer": "HH", "was": "Standing", "wieviel": 100}]"#),
            )]),
            ..Table::default()
        };
        assert_eq!(line(&table, "t", "Ruf"), "# Ruf: CFP -50 Standing, HH +100 Standing");
        let span = Table {
            contracts: merge_parties(&[
                ("s".into(), rows(r#"[{"wer": "CFP", "was": "Standing", "wieviel": -50}]"#)),
                ("s".into(), rows(r#"[{"wer": "CFP", "was": "Standing", "wieviel": 20}]"#)),
            ]),
            ..Table::default()
        };
        assert_eq!(line(&span, "s", "Ruf"), "# Ruf: CFP -50–20 Standing");
    }

    #[test]
    fn lines_from_the_table() {
        let table = Table {
            contracts: jsonio::loads(
                r#"{"k_title": [{"wer": "Headhunters", "was": "Standing", "wieviel": 50}, {"wer": "", "was": "Rep", "wieviel": 5}]}"#,
            )
            .unwrap()
            .as_object()
            .unwrap()
            .clone(),
            regions: vec![("k_title".into(), Value::str("Pyro (Bloom)"))],
            ..Table::default()
        };
        assert_eq!(line(&table, "@K_Title", "Ruf"), "# Ruf: Headhunters +50 Standing, +5 Rep");
        assert_eq!(region_line(&table, "K_TITLE", "Region"), "# Region: Pyro (Bloom)");
        assert_eq!(line(&table, "other", "Ruf"), "");
        assert!(same_build("4.10.1-live.1", "4.10.1"));
        assert!(!same_build("4.10.10-live.1", "4.10.1"));
    }
}
