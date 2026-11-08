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

//! What a game patch changed in the values, kept for good (`patch_changes.py` of 4.12), and how a changed value
//! is written (`_patch_number`, `_pc_pair`, `_pc_percent`, `_pc_direction`, `_pc_struct` of `pages.py`).
//!
//! Not to be mixed up with `patch_history`: that one says which **blueprints** a patch brought (own
//! observation), this one which **values** it changed (erkul's finished diffs).
//!
//! erkul keeps only the last ten patches. Each patch with changes is kept in a file of its own
//! (`Patches/patch-<version>.json`, compact), so the collection grows beyond the source. Patches without a
//! changed value (most hotfixes) are neither kept nor fetched: the summary in erkul's table of contents tells
//! them. ⚠ The path of a changelog needs the branch in front (`LIVE/changelog.….bin`), else the answer is
//! empty without any message.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use crate::erkul_fetch::{self, Feed};
use crate::jsonio::{self, Map, Value};
use crate::language::{Arg, t, tf};
use crate::paths::Paths;

/// The subfolder of the patch files.
pub const FOLDER: &str = "Patches";
/// At most this many items are drawn at once (`_PATCH_MAX`).
pub const SHOWN_AT_MOST: usize = 60;
/// What the source says of a patch (`unchanged` is no content and not counted).
const COUNTER: [&str; 3] = ["added", "removed", "modified"];

/// The file name of a version: only digits, letters, dot and dash; anything else becomes `_` (the version
/// comes from the net and must not lead out of the folder).
pub fn safe_name(version: &str) -> String {
    let version = if version.is_empty() { "unbekannt" } else { version };
    version.chars().map(|c| if c.is_ascii_alphanumeric() || c == '.' || c == '-' { c } else { '_' }).collect()
}

/// The folder of the patch files: `Patches` in the data folder, flat in a data folder given by `SC_BP_HOME`.
pub fn folder(paths: &Paths) -> String {
    let base = paths.app_folder();
    if paths.host.var_set(crate::legacy::ENV_HOME).is_some() {
        return base;
    }
    let target = paths.host.flavor.join(&base, FOLDER);
    if jsonio::make_dirs(&target).is_err() {
        return base;
    }
    target
}

fn file(paths: &Paths, version: &str) -> String {
    paths.host.flavor.join(&folder(paths), &format!("patch-{}.json", safe_name(version)))
}

/// A number of a summary; anything else counts as 0 (`_number`).
fn count(value: Option<&Value>) -> i64 {
    match value {
        Some(Value::Int(text)) => text.parse().unwrap_or(0),
        Some(Value::Float(x)) if x.is_finite() => x.trunc() as i64,
        Some(Value::Bool(true)) => 1,
        Some(Value::Str(text)) => text.trim().parse().unwrap_or(0),
        _ => 0,
    }
}

/// What a patch changed: new, gone, changed items.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Summary {
    pub added: i64,
    pub removed: i64,
    pub modified: i64,
}

impl Summary {
    /// The summary as erkul writes it (`{"added": 3, …}`); anything missing or not a number counts 0.
    pub fn of(value: Option<&Value>) -> Summary {
        let map = value.and_then(Value::as_object);
        let get = |key: &str| count(map.and_then(|m| m.iter().find(|(k, _)| k == key)).map(|(_, v)| v));
        Summary { added: get(COUNTER[0]), removed: get(COUNTER[1]), modified: get(COUNTER[2]) }
    }

    /// The patch changed something (`_has_content`).
    pub fn has_content(&self) -> bool {
        self.added != 0 || self.removed != 0 || self.modified != 0
    }
}

/// A patch erkul offers now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Offered {
    pub version: String,
    /// `YYYY-MM-DD`.
    pub date: String,
    pub summary: Summary,
    pub path: String,
}

fn text(map: &Map, key: &str) -> String {
    map.iter().find(|(k, _)| k == key).and_then(|(_, v)| v.as_str()).unwrap_or("").to_string()
}

fn field<'a>(map: &'a Map, key: &str) -> Option<&'a Value> {
    map.iter().find(|(k, _)| k == key).map(|(_, v)| v)
}

/// Newest first, by the numbers of the version.
fn newest_first<T>(list: &mut [T], version: impl Fn(&T) -> &str) {
    list.sort_by_key(|item| std::cmp::Reverse(crate::patch_history::rank(version(item))));
}

/// What erkul offers now, newest first (`offered`); `None` when its table of contents cannot be had.
pub fn offered(feed: &dyn Feed) -> Option<Vec<Offered>> {
    let catalog = erkul_fetch::ship_catalog(feed)?;
    let mut out: Vec<Offered> = match catalog.get("patches") {
        Some(Value::Array(patches)) => patches
            .iter()
            .filter_map(Value::as_object)
            .filter_map(|p| {
                let version = text(p, "dataVersion");
                (!version.is_empty()).then(|| Offered {
                    date: text(p, "generatedAt").chars().take(10).collect(),
                    summary: Summary::of(field(p, "summary")),
                    path: text(p, "path"),
                    version,
                })
            })
            .collect(),
        _ => Vec::new(),
    };
    newest_first(&mut out, |o| &o.version);
    Some(out)
}

fn read_file(file: &str) -> Option<Map> {
    match jsonio::read_json(file) {
        Ok(Value::Object(map)) => Some(map),
        _ => None,
    }
}

/// The file names of the patch files.
fn patch_files(paths: &Paths) -> Vec<String> {
    let folder = folder(paths);
    let Ok(entries) = std::fs::read_dir(&folder) else { return Vec::new() };
    let mut names: Vec<String> = entries
        .filter_map(Result::ok)
        .filter_map(|e| e.file_name().to_str().map(str::to_string))
        .filter(|name| name.starts_with("patch-") && name.ends_with(".json"))
        .collect();
    names.sort();
    names
}

/// The versions kept here, newest first (`stored`).
pub fn stored(paths: &Paths) -> Vec<String> {
    let folder = folder(paths);
    let mut versions: Vec<String> = patch_files(paths)
        .iter()
        .filter_map(|name| read_file(&paths.host.flavor.join(&folder, name)))
        .map(|data| text(&data, "version"))
        .filter(|v| !v.is_empty())
        .collect();
    newest_first(&mut versions, |v| v);
    versions
}

/// Name, size and time of every patch file: changes when one is added, goes or changes (`stamp`).
pub fn stamp(paths: &Paths) -> Vec<(String, u64, u128)> {
    let folder = folder(paths);
    patch_files(paths)
        .into_iter()
        .map(|name| {
            let meta = std::fs::metadata(paths.host.flavor.join(&folder, &name)).ok();
            let size = meta.as_ref().map_or(0, std::fs::Metadata::len);
            let time = meta
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map_or(0, |d| d.as_nanos());
            (name, size, time)
        })
        .collect()
}

/// The changelog kept of a version (`load`).
pub fn load(paths: &Paths, version: &str) -> Option<Map> {
    read_file(&file(paths, version))
}

/// Why fetching did not work.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Unreachable {
    /// erkul's table of contents could not be had.
    Source,
}

/// Fetches what erkul offers and what is missing here, only patches with changes, and keeps them (`sync`).
/// The versions kept anew; an empty patch is not fetched, a patch whose changelog cannot be had is left for the
/// next time.
///
/// # Errors
///
/// [`Unreachable::Source`] when erkul's table of contents cannot be had (no net).
pub fn sync(paths: &Paths, feed: &dyn Feed) -> Result<Vec<String>, Unreachable> {
    let present: HashSet<String> = stored(paths).into_iter().collect();
    let offer = offered(feed).ok_or(Unreachable::Source)?;
    let mut new = Vec::new();
    for entry in offer {
        if present.contains(&entry.version) || !entry.summary.has_content() || entry.path.is_empty() {
            continue;
        }
        let Some(Value::Object(mut data)) =
            erkul_fetch::fetch(feed, &format!("{}/{}", erkul_fetch::BRANCH, entry.path))
        else {
            continue;
        };
        jsonio::map_set(&mut data, "version", Value::str(&entry.version));
        jsonio::map_set(&mut data, "datum", Value::str(&entry.date));
        jsonio::map_set(&mut data, "quelle", Value::str("erkul.games"));
        let target = file(paths, &entry.version);
        let written = jsonio::write_text_atomic(&target, &compact(&Value::Object(data)), jsonio::Newline::Keep);
        match written {
            Ok(()) => new.push(entry.version),
            Err(error) if crate::writeguard::refused(&error) => {}
            Err(error) => crate::errors::record("patch_changes._write", &error),
        }
    }
    Ok(new)
}

/// JSON without spaces between the parts (`separators=(',', ':')`): nobody reads the file by hand.
fn compact(value: &Value) -> String {
    let mut out = String::new();
    write_compact(&mut out, value);
    out
}

fn write_compact(out: &mut String, value: &Value) {
    match value {
        Value::Array(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_compact(out, item);
            }
            out.push(']');
        }
        Value::Object(map) => {
            out.push('{');
            for (i, (key, item)) in map.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push_str(&jsonio::dumps(&Value::str(key), None, false));
                out.push(':');
                write_compact(out, item);
            }
            out.push('}');
        }
        other => out.push_str(&jsonio::dumps(other, None, false)),
    }
}

/// One patch of the list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Patch {
    pub version: String,
    /// The version without the build (`4.10.0`).
    pub short: String,
    pub date: String,
    pub summary: Summary,
    /// No value changed.
    pub empty: bool,
    /// erkul offers it now.
    pub at_source: bool,
    /// Kept here.
    pub kept: bool,
}

/// Everything there is to show, newest first (`overview`): what erkul offers and what is kept here, so a patch
/// erkul dropped stays, marked as not at the source.
pub fn overview(paths: &Paths, offered: &[Offered]) -> Vec<Patch> {
    // In the order they come, so that versions of the same rank keep it.
    let mut combined: Vec<Patch> = Vec::new();
    for entry in offered {
        let patch = Patch {
            version: entry.version.clone(),
            short: entry.version.split('-').next().unwrap_or_default().to_string(),
            date: entry.date.clone(),
            summary: entry.summary.clone(),
            empty: !entry.summary.has_content(),
            at_source: true,
            kept: false,
        };
        match combined.iter_mut().find(|p| p.version == entry.version) {
            Some(known) => *known = patch,
            None => combined.push(patch),
        }
    }
    for version in stored(paths) {
        if let Some(known) = combined.iter_mut().find(|p| p.version == version) {
            known.kept = true;
            continue;
        }
        let data = load(paths, &version).unwrap_or_default();
        combined.push(Patch {
            short: version.split('-').next().unwrap_or_default().to_string(),
            date: text(&data, "datum"),
            summary: Summary::of(field(&data, "summary")),
            empty: false,
            at_source: false,
            kept: true,
            version,
        });
    }
    newest_first(&mut combined, |p| &p.version);
    combined
}

/// What happened to an item.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fate {
    New,
    Gone,
    Changed,
}

/// One changed field: old and new value, each of which may be missing (and that is the news: a field that
/// went or came).
#[derive(Clone, Debug, PartialEq)]
pub struct FieldChange {
    pub path: String,
    pub old: Option<Value>,
    pub new: Option<Value>,
}

/// One item of a patch.
#[derive(Clone, Debug, PartialEq)]
pub struct Change {
    /// The area of erkul (`ships`, `weapons` …).
    pub kind: String,
    pub fate: Fate,
    pub id: String,
    pub name: String,
    pub size: Option<Value>,
    pub fields: Vec<FieldChange>,
}

/// The changes of a patch, flat (`changes`); with `wanted` only those of that area.
pub fn changes(paths: &Paths, version: &str, wanted: Option<&str>) -> Vec<Change> {
    let Some(data) = load(paths, version) else { return Vec::new() };
    let mut out = Vec::new();
    let Some(Value::Array(categories)) = field(&data, "categories") else { return out };
    for category in categories.iter().filter_map(Value::as_object) {
        let kind = text(category, "kind");
        if wanted.is_some_and(|w| !w.is_empty() && w != kind) {
            continue;
        }
        for (fate, key) in [(Fate::New, "added"), (Fate::Gone, "removed"), (Fate::Changed, "modified")] {
            // ⚠ `unchanged` is a number, the other three are lists.
            let Some(Value::Array(items)) = field(category, key) else { continue };
            for item in items.iter().filter_map(Value::as_object) {
                let fields = match field(item, "changes") {
                    Some(Value::Array(list)) => list
                        .iter()
                        .filter_map(Value::as_object)
                        .map(|change| FieldChange {
                            path: text(change, "path"),
                            old: field(change, "oldValue").cloned(),
                            new: field(change, "newValue").cloned(),
                        })
                        .collect(),
                    _ => Vec::new(),
                };
                let first =
                    |keys: &[&str]| keys.iter().map(|k| text(item, k)).find(|v| !v.is_empty()).unwrap_or_default();
                out.push(Change {
                    kind: kind.clone(),
                    fate,
                    id: text(item, "id"),
                    name: first(&["name", "className", "id"]),
                    size: field(item, "size").filter(|v| !matches!(v, Value::Null)).cloned(),
                    fields,
                });
            }
        }
    }
    out
}

/// The areas with changes and how many items each, the largest first (`categories`).
pub fn categories(changes: &[Change]) -> Vec<(String, usize)> {
    let mut counter: BTreeMap<&str, usize> = BTreeMap::new();
    for change in changes {
        *counter.entry(change.kind.as_str()).or_default() += 1;
    }
    let mut out: Vec<(String, usize)> = counter.into_iter().map(|(k, n)| (k.to_string(), n)).collect();
    out.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    out
}

// ------------------------------------------------------------------ Writing a value

/// Python's `%g`: six significant digits, the exponent from 1e6 up and below 1e-4.
fn general(x: f64) -> String {
    if x == 0.0 {
        return if x.is_sign_negative() { "-0".into() } else { "0".into() };
    }
    if !x.is_finite() {
        return if x.is_nan() {
            "nan".into()
        } else if x > 0.0 {
            "inf".into()
        } else {
            "-inf".into()
        };
    }
    let scientific = format!("{x:.5e}");
    let (mantissa, exponent) = scientific.split_once('e').unwrap_or((&scientific, "0"));
    let exponent: i32 = exponent.parse().unwrap_or(0);
    if (-4..6).contains(&exponent) {
        let places = usize::try_from(5 - exponent).unwrap_or(0);
        strip_zeros(&format!("{x:.places$}"))
    } else {
        let mantissa = strip_zeros(mantissa);
        let sign = if exponent < 0 { '-' } else { '+' };
        format!("{mantissa}e{sign}{:02}", exponent.abs())
    }
}

fn strip_zeros(text: &str) -> String {
    if text.contains('.') { text.trim_end_matches('0').trim_end_matches('.').to_string() } else { text.to_string() }
}

/// Python's `round(x, places)`.
fn round_to(x: f64, places: usize) -> f64 {
    format!("{x:.places$}").parse().unwrap_or(x)
}

/// A value of the patch data as it is written (`_patch_number`): floats rounded to `places` without the
/// exponent below 0.0001, truth as yes or no, nested values as `…`.
pub fn number_text(value: Option<&Value>, places: usize) -> String {
    match value {
        None | Some(Value::Null) => "—".into(),
        Some(Value::Bool(on)) => t(if *on { "s_pa_ja" } else { "s_pa_nein" }),
        Some(Value::Float(x)) => {
            let rounded = round_to(*x, places);
            if rounded != 0.0 && rounded.abs() < 1e-4 {
                strip_zeros(&format!("{rounded:.places$}"))
            } else {
                general(rounded)
            }
        }
        Some(Value::Int(text) | Value::Str(text)) => text.clone(),
        Some(Value::Array(_) | Value::Object(_)) => "…".into(),
    }
}

/// A number that is not a truth value.
fn real(value: Option<&Value>) -> Option<f64> {
    match value? {
        Value::Int(text) => text.parse().ok(),
        Value::Float(x) => Some(*x),
        _ => None,
    }
}

/// Old and new written so that a difference shows (`_pc_pair`): the places grow until the two differ and
/// neither number reads as a zero it is not.
pub fn pair(old: Option<&Value>, new: Option<&Value>) -> (String, String) {
    let swallowed = |raw: Option<&Value>, shown: &str| shown == "0" && real(raw).is_some_and(|x| x != 0.0);
    for places in [4, 6, 8, 10, 12, 14] {
        let (before, after) = (number_text(old, places), number_text(new, places));
        if before != after && !swallowed(old, &before) && !swallowed(new, &after) {
            return (before, after);
        }
    }
    (number_text(old, 4), number_text(new, 4))
}

/// How much the value changed, as an addition (`_pc_percent`): `  (+9 %)`, from three times on `  (×3.5)`;
/// nothing without a base, for truth values and texts, and under half a percent.
pub fn percent(old: Option<&Value>, new: Option<&Value>) -> String {
    let (Some(a), Some(b)) = (real(old), real(new)) else { return String::new() };
    if a == 0.0 || a == b {
        return String::new();
    }
    let share = (b - a) / a.abs() * 100.0;
    if share.abs() < 0.5 {
        return String::new();
    }
    if share >= 200.0 {
        return format!("  (×{})", number_text(Some(&Value::Float(round_to(b / a.abs(), 1))), 4));
    }
    format!("  ({:+} %)", share.round_ties_even() as i64)
}

/// Whether the value rose (1), fell (-1) or neither (0) (`_pc_direction`); only numbers, not truth values.
pub fn direction(old: Option<&Value>, new: Option<&Value>) -> i8 {
    match (real(old), real(new)) {
        (Some(a), Some(b)) if b > a => 1,
        (Some(a), Some(b)) if b < a => -1,
        _ => 0,
    }
}

/// Every single value of a nested one, with its path (`_pc_leaves`).
fn leaves(value: &Value, path: &str, out: &mut Vec<(String, Value)>) {
    match value {
        Value::Object(map) => {
            for (key, inner) in map {
                let below = if path.is_empty() { key.clone() } else { format!("{path}.{key}") };
                leaves(inner, &below, out);
            }
        }
        Value::Array(items) => {
            for (number, inner) in items.iter().enumerate() {
                leaves(inner, &format!("{path}[{number}]"), out);
            }
        }
        other => {
            if let Some(at) = out.iter().position(|(p, _)| p == path) {
                out[at].1 = other.clone();
            } else {
                out.push((path.to_string(), other.clone()));
            }
        }
    }
}

/// The speaking end of a leaf path, or its name in the field table (`_pc_short_path`).
fn short_path(path: &str) -> String {
    let readable = crate::language::pa_field(path);
    if readable != path {
        return readable;
    }
    path.rsplit('.').next().filter(|last| !last.is_empty()).unwrap_or(path).to_string()
}

/// What really differs between two nested values (`_pc_struct`): at most `at_most` leaves named, the rest
/// counted; „umgebaut, kein Wert anders“ when no leaf differs.
pub fn struct_diff(old: &Value, new: &Value, at_most: usize) -> String {
    let (mut a, mut b) = (Vec::new(), Vec::new());
    leaves(old, "", &mut a);
    leaves(new, "", &mut b);
    let get = |list: &[(String, Value)], key: &str| list.iter().find(|(p, _)| p == key).map(|(_, v)| v.clone());
    let keys: BTreeSet<&String> = a.iter().chain(b.iter()).map(|(p, _)| p).collect();
    let mut parts = Vec::new();
    for key in keys {
        let (x, y) = (get(&a, key), get(&b, key));
        if jsonio::py_eq(x.as_ref(), y.as_ref()) {
            continue;
        }
        let short = short_path(key);
        match (x, y) {
            (_, None) => parts.push(tf("s_pa_feld_weg", &[("feld", Arg::from(short))])),
            (None, Some(y)) => parts.push(tf(
                "s_pa_feld_neu",
                &[("feld", Arg::from(short)), ("wert", Arg::from(number_text(Some(&y), 4)))],
            )),
            (Some(x), Some(y)) => {
                parts.push(format!("{short} {} → {}", number_text(Some(&x), 4), number_text(Some(&y), 4)));
            }
        }
    }
    if parts.is_empty() {
        return t("s_pa_gleich");
    }
    if parts.len() > at_most {
        let rest = parts.len() - at_most;
        return format!("{}, {}", parts[..at_most].join(", "), tf("s_pa_mehr", &[("n", Arg::from(rest))]));
    }
    parts.join(", ")
}

/// How a changed field reads: the text and its tone.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Tone {
    /// Rose, or a field that came.
    Up,
    /// Fell.
    Down,
    /// A field that went.
    Gone,
    /// The value the same.
    Same,
    Plain,
}

/// The text of one changed field and its tone (`_pc_field_row`).
pub fn field_text(change: &FieldChange) -> (String, Tone) {
    let (old, new) = (change.old.as_ref(), change.new.as_ref());
    match (old, new) {
        (Some(o), Some(n)) => {
            if matches!(o, Value::Array(_) | Value::Object(_)) || matches!(n, Value::Array(_) | Value::Object(_)) {
                return (struct_diff(o, n, 3), Tone::Plain);
            }
            let (before, after) = pair(old, new);
            if before == after {
                return (format!("{before} → {after}"), Tone::Same);
            }
            let tone = match direction(old, new) {
                1 => Tone::Up,
                -1 => Tone::Down,
                _ => Tone::Plain,
            };
            (format!("{before} → {after}{}", percent(old, new)), tone)
        }
        (Some(o), None) => (tf("s_pa_weggefallen", &[("alt", Arg::from(number_text(Some(o), 4)))]), Tone::Gone),
        (None, n) => (tf("s_pa_dazugekommen", &[("neu", Arg::from(number_text(n, 4)))]), Tone::Up),
    }
}

/// How many items of a kept patch stayed the same: the `unchanged` of its summary, else the sum of its areas;
/// `None` when the file says neither.
pub fn unchanged(paths: &Paths, version: &str) -> Option<i64> {
    let data = load(paths, version)?;
    let said = |value: Option<&Value>| value.filter(|v| !matches!(v, Value::Null)).map(|v| count(Some(v)));
    let from_summary = field(&data, "summary").and_then(Value::as_object).and_then(|s| said(field(s, "unchanged")));
    if from_summary.is_some() {
        return from_summary;
    }
    // The sum of the areas only when every area says it: a part would read as the whole.
    let Some(Value::Array(categories)) = field(&data, "categories") else { return None };
    let numbers: Option<Vec<i64>> =
        categories.iter().filter_map(Value::as_object).map(|c| said(field(c, "unchanged"))).collect();
    numbers.filter(|n| !n.is_empty()).map(|n| n.iter().sum())
}

/// One number of a patch that rose or fell, with how much in percent of the old value.
#[derive(Clone, Debug, PartialEq)]
pub struct Move {
    pub kind: String,
    pub name: String,
    pub path: String,
    pub old: Value,
    pub new: Value,
    /// `(new - old) / old * 100`, the old value above 0.
    pub share: f64,
}

/// The numbers of the changed items that rose most and that fell most, each at most `at_most`, the largest first.
/// Only numbers with an old value above 0 count (a share of a negative base reads wrong), and only a change of half
/// a percent and more (what [`percent`] writes); truth values, texts, nested values and new items do not.
pub fn largest_moves(changes: &[Change], at_most: usize) -> (Vec<Move>, Vec<Move>) {
    let mut moves: Vec<Move> = Vec::new();
    for change in changes.iter().filter(|c| c.fate == Fate::Changed) {
        for field in &change.fields {
            let (Some(a), Some(b)) = (real(field.old.as_ref()), real(field.new.as_ref())) else { continue };
            if a <= 0.0 || !b.is_finite() {
                continue;
            }
            let share = (b - a) / a * 100.0;
            // A tiny base can make the share endless; it has no length a bar could show.
            if !share.is_finite() || share.abs() < 0.5 {
                continue;
            }
            let (Some(old), Some(new)) = (field.old.clone(), field.new.clone()) else { continue };
            moves.push(Move {
                kind: change.kind.clone(),
                name: change.name.clone(),
                path: field.path.clone(),
                old,
                new,
                share,
            });
        }
    }
    // A stable sort: of two equal moves the one listed first stays first.
    moves.sort_by(|x, y| y.share.abs().total_cmp(&x.share.abs()));
    let (mut rises, mut falls): (Vec<Move>, Vec<Move>) = moves.into_iter().partition(|m| m.share > 0.0);
    rises.truncate(at_most);
    falls.truncate(at_most);
    (rises, falls)
}

/// How much a value changed, as a short text: `+20 %`, from three times on `×3.5`; empty as with [`percent`].
pub fn share_text(old: Option<&Value>, new: Option<&Value>) -> String {
    percent(old, new).trim().trim_start_matches('(').trim_end_matches(')').to_string()
}

/// The file of the patches already looked at.
pub const SEEN_FILE: &str = "patches-gelesen.json";

fn seen_file(paths: &Paths) -> String {
    paths.host.flavor.join(&paths.app_folder(), SEEN_FILE)
}

/// Whether the file of the patches looked at is there, read or not.
pub fn seen_exists(paths: &Paths) -> bool {
    std::path::Path::new(&seen_file(paths)).exists()
}

/// The versions already looked at; `None` before the page was ever opened with patches in its list, or when the
/// file cannot be read.
pub fn seen(paths: &Paths) -> Option<BTreeSet<String>> {
    let map = read_file(&seen_file(paths))?;
    let versions = match field(&map, "gelesen") {
        Some(Value::Array(list)) => list.iter().filter_map(Value::as_str).map(str::to_string).collect(),
        _ => BTreeSet::new(),
    };
    Some(versions)
}

/// Keeps the versions looked at.
///
/// # Errors
///
/// When the file cannot be written.
pub fn set_seen(paths: &Paths, versions: &BTreeSet<String>) -> std::io::Result<()> {
    let list = versions.iter().map(|v| Value::str(v.as_str())).collect();
    let mut map = Map::new();
    jsonio::map_set(&mut map, "gelesen", Value::Array(list));
    let text = jsonio::dumps(&Value::Object(map), Some(2), false);
    jsonio::write_text_atomic(&seen_file(paths), &text, jsonio::Newline::Keep)
}

/// The patches of the list with changes that were not looked at yet, newest first.
pub fn unread(list: &[Patch], seen: &BTreeSet<String>) -> Vec<String> {
    list.iter().filter(|p| !p.empty && !seen.contains(&p.version)).map(|p| p.version.clone()).collect()
}

/// The counter of a patch: „3 neu · 0 weg · 12 geändert“.
pub fn counter_text(summary: &Summary) -> String {
    tf(
        "s_pa_zaehler",
        &[
            ("plus", Arg::from(summary.added)),
            ("minus", Arg::from(summary.removed)),
            ("tilde", Arg::from(summary.modified)),
        ],
    )
}

#[cfg(test)]
mod tests;
