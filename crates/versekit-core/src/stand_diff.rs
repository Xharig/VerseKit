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

//! The comparison of two stands (`stands`): what a patch changed in what the own readers see, written in the form
//! of erkul's patches, so that the page of the changed values shows it without any change of its drawing.
//!
//! - Every changed value is one of five kinds: the value (a number or truth value), the text, the form (a number
//!   turned into a range, a field that came or went, a text turned into a placeholder), a new entry, a gone entry.
//! - A change that hits many entries alike (same area, path, old and new value) is one row with a count and at most
//!   [`EXAMPLES`] examples; without it a renamed rank would fill the page.
//! - A counter of the reading probe that grew, or an area that vanished, is a warning.
//! - Stands of two readers are compared in the fields both have; the rest is one row of the kind form, with the
//!   note that the reader changed.
//! - ⚠ The result of a channel under an agreement is confidential like its stands: it is written only where the program is built
//!   for the channel.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use crate::jsonio::{self, Map, Value};
use crate::patch_changes::Fate;
use crate::paths::Paths;
use crate::stands::{self, Area, Entry, Leaf, Stand};

/// At most this many examples of a change shared by many entries.
pub const EXAMPLES: usize = 3;
/// A change is a shared row from this many entries on.
pub const SHARED_FROM: usize = 2;
/// The areas that are a scale (levels and ceiling) and get a line of their own.
pub const SCALE_AREAS: [&str; 1] = ["rufstufen.stufen"];
/// What the patch file says of its source.
pub const SOURCE: &str = "eigen";
/// The areas whose name begins so are only counted, not listed (the texts: a checksum per key).
pub const COUNT_ONLY: &str = "texte.";

/// What kind a change is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    Value,
    Text,
    Shape,
    New,
    Gone,
}

impl Kind {
    /// The name in the patch file.
    pub fn key(self) -> &'static str {
        match self {
            Kind::Value => "wert",
            Kind::Text => "text",
            Kind::Shape => "form",
            Kind::New => "neu",
            Kind::Gone => "weg",
        }
    }

    /// How much the kind says when a row holds several: the form first.
    fn weight(self) -> u8 {
        match self {
            Kind::Shape => 3,
            Kind::Text => 2,
            Kind::Value => 1,
            Kind::New | Kind::Gone => 0,
        }
    }
}

/// How serious a finding is: breaks (a reader fails, a counter jumps), looks different (form, text, scale), only
/// new or changed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Class {
    Breaks,
    LooksDifferent,
    NewOrChanged,
}

/// One changed value of an entry; a missing side is a field that came or went.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct FieldDiff {
    pub path: String,
    pub old: Option<Leaf>,
    pub new: Option<Leaf>,
    pub kind: Kind,
}

/// One row of the comparison.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    pub area: String,
    pub fate: Fate,
    /// The key of the first entry.
    pub id: String,
    pub name: String,
    /// How many entries the row stands for.
    pub count: usize,
    /// At most [`EXAMPLES`] keys of the entries.
    pub examples: Vec<String>,
    pub changes: Vec<FieldDiff>,
    /// A field of one reader only: the reader changed, not the game.
    pub reader_note: bool,
}

impl Row {
    pub fn kind(&self) -> Kind {
        match self.fate {
            Fate::New => Kind::New,
            Fate::Gone => Kind::Gone,
            Fate::Changed => self.changes.iter().map(|c| c.kind).max_by_key(|k| k.weight()).unwrap_or(Kind::Value),
        }
    }

    pub fn class(&self) -> Class {
        if self.reader_note || self.kind() == Kind::Shape { Class::LooksDifferent } else { Class::NewOrChanged }
    }
}

/// The numbers of an area.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AreaCount {
    pub area: String,
    pub old: usize,
    pub new: usize,
    pub added: usize,
    pub removed: usize,
    pub changed: usize,
    pub unchanged: usize,
}

/// A scale: how many levels and where it ends.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Scale {
    pub levels: usize,
    pub ceiling: Option<i64>,
}

/// A scale of both stands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScaleRow {
    pub area: String,
    pub old: Option<Scale>,
    pub new: Option<Scale>,
}

impl ScaleRow {
    pub fn changed(&self) -> bool {
        self.old != self.new
    }
}

/// A gap that grew or an area that vanished.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Warning {
    pub name: String,
    pub old: i64,
    pub new: i64,
    pub class: Class,
}

/// Which stand a side of the comparison is.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Meta {
    pub channel: String,
    pub build: String,
    pub date: String,
    pub reader: String,
    pub confidential: bool,
    pub under_agreement: bool,
}

impl Meta {
    fn of(stand: &Stand) -> Meta {
        Meta {
            channel: stand.channel.clone(),
            build: stand.build.clone(),
            date: stand.date.clone(),
            reader: stand.reader.clone(),
            confidential: stand.confidential,
            under_agreement: stand.under_agreement,
        }
    }
}

/// How many findings of each class.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Classes {
    pub breaks: usize,
    pub looks_different: usize,
    pub new_or_changed: usize,
}

/// What two stands differ in.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Comparison {
    pub from: Meta,
    pub to: Meta,
    /// The two stands come from readers of another version: only the fields both have are compared.
    pub reader_changed: bool,
    pub counts: Vec<AreaCount>,
    pub scales: Vec<ScaleRow>,
    pub rows: Vec<Row>,
    pub warnings: Vec<Warning>,
}

impl Comparison {
    /// Nothing differs and nothing grew.
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
            && self.warnings.is_empty()
            && !self.scales.iter().any(ScaleRow::changed)
            && !self.counts.iter().any(|c| c.added + c.removed + c.changed > 0)
    }

    /// How many findings fall in each class (a row counts once, however many entries it stands for).
    pub fn classes(&self) -> Classes {
        let mut classes = Classes::default();
        let mut add = |class: Class| match class {
            Class::Breaks => classes.breaks += 1,
            Class::LooksDifferent => classes.looks_different += 1,
            Class::NewOrChanged => classes.new_or_changed += 1,
        };
        self.warnings.iter().for_each(|w| add(w.class));
        self.scales.iter().filter(|s| s.changed()).for_each(|_| add(Class::LooksDifferent));
        self.rows.iter().for_each(|r| add(r.class()));
        classes
    }

    /// The entries new, gone and changed over all areas.
    pub fn summary(&self) -> (usize, usize, usize) {
        let listed = self.counts.iter().filter(|n| !n.area.starts_with(COUNT_ONLY));
        listed.fold((0, 0, 0), |(a, r, c), n| (a + n.added, r + n.removed, c + n.changed))
    }
}

/// The path without the places of the lists: `slots[3].size` and `slots[0].size` are the same field.
fn shape_of(path: &str) -> String {
    let mut out = String::with_capacity(path.len());
    let mut inside = false;
    for c in path.chars() {
        match c {
            '[' => {
                inside = true;
                out.push('[');
            }
            ']' => {
                inside = false;
                out.push(']');
            }
            c if !inside => out.push(c),
            _ => {}
        }
    }
    out
}

fn shapes_of(area: &Area) -> HashSet<String> {
    area.entries.values().flat_map(|entry| entry.keys().map(|path| shape_of(path))).collect()
}

fn entry_name(key: &str, entry: &Entry) -> String {
    ["name", "titel", "title"]
        .iter()
        .find_map(|path| match entry.get(*path) {
            Some(Leaf::Text(text)) if !text.is_empty() => Some(text.clone()),
            _ => None,
        })
        .unwrap_or_else(|| key.to_string())
}

fn kind_of(old: &Leaf, new: &Leaf) -> Kind {
    match (old, new) {
        (a, b) if a.is_countable() && b.is_countable() => Kind::Value,
        (Leaf::Text(a), Leaf::Text(b)) if stands::is_placeholder(b) && !stands::is_placeholder(a) => Kind::Shape,
        (Leaf::Text(_), Leaf::Text(_)) => Kind::Text,
        _ => Kind::Shape,
    }
}

/// The values below a path written as one text: `[1, 60]` for a list, `{a: 1}` for an object.
fn composed(parent: &str, children: &[&FieldDiff], from_new: bool) -> Leaf {
    let mut parts: Vec<(&str, String)> = children
        .iter()
        .filter_map(|c| {
            let leaf = if from_new { c.new.as_ref() } else { c.old.as_ref() }?;
            Some((c.path.strip_prefix(parent)?, leaf.show()))
        })
        .collect();
    parts.sort_by_key(|(rest, _)| {
        let index: Option<usize> = rest.strip_prefix('[').and_then(|r| r.split(']').next()).and_then(|n| n.parse().ok());
        (index.unwrap_or(usize::MAX), rest.to_string())
    });
    let list = parts.iter().all(|(rest, _)| rest.starts_with('['));
    let body: Vec<String> =
        parts.iter().map(|(rest, value)| if list { value.clone() } else { format!("{}: {value}", rest.trim_start_matches('.')) }).collect();
    Leaf::Text(if list { format!("[{}]", body.join(", ")) } else { format!("{{{}}}", body.join(", ")) })
}

/// A value that went while values below its path came (or the other way round) is one change of the form.
fn merge_forms(diffs: Vec<FieldDiff>) -> Vec<FieldDiff> {
    let mut used = vec![false; diffs.len()];
    let mut merged = Vec::new();
    for (i, parent) in diffs.iter().enumerate() {
        let went = parent.old.is_some() && parent.new.is_none();
        let came = parent.old.is_none() && parent.new.is_some();
        if used[i] || parent.path.is_empty() || !(went || came) {
            continue;
        }
        let below = |c: &FieldDiff| {
            let inside = c.path.starts_with(&format!("{}[", parent.path)) || c.path.starts_with(&format!("{}.", parent.path));
            inside && if went { c.old.is_none() && c.new.is_some() } else { c.old.is_some() && c.new.is_none() }
        };
        let children: Vec<usize> = (0..diffs.len()).filter(|&j| j != i && !used[j] && below(&diffs[j])).collect();
        if children.is_empty() {
            continue;
        }
        let refs: Vec<&FieldDiff> = children.iter().map(|&j| &diffs[j]).collect();
        let whole = composed(&parent.path, &refs, went);
        let (old, new) = if went { (parent.old.clone(), Some(whole)) } else { (Some(whole), parent.new.clone()) };
        merged.push(FieldDiff { path: parent.path.clone(), old, new, kind: Kind::Shape });
        used[i] = true;
        children.iter().for_each(|&j| used[j] = true);
    }
    let mut out: Vec<FieldDiff> = diffs.into_iter().zip(used).filter(|(_, used)| !used).map(|(d, _)| d).collect();
    out.extend(merged);
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out
}

/// The shapes of the paths each side has, when two readers are compared.
struct Common<'a> {
    old: &'a HashSet<String>,
    new: &'a HashSet<String>,
}

/// The changed values of one entry, and the fields only one reader has.
fn diff_entry(old: &Entry, new: &Entry, common: Option<&Common>, notes: &mut Vec<String>) -> Vec<FieldDiff> {
    let paths: BTreeSet<&String> = old.keys().chain(new.keys()).collect();
    let mut diffs = Vec::new();
    for path in paths {
        let (a, b) = (old.get(path), new.get(path));
        let kind = match (a, b) {
            (Some(a), Some(b)) if a.same_as(b) => continue,
            (Some(a), Some(b)) => kind_of(a, b),
            _ => Kind::Shape,
        };
        diffs.push(FieldDiff { path: path.clone(), old: a.cloned(), new: b.cloned(), kind });
    }
    let diffs = merge_forms(diffs);
    let Some(common) = common else { return diffs };
    diffs
        .into_iter()
        .filter(|d| {
            let shape = shape_of(&d.path);
            let only_old = d.new.is_none() && !common.new.contains(&shape);
            let only_new = d.old.is_none() && !common.old.contains(&shape);
            if only_old || only_new {
                notes.push(shape);
            }
            !(only_old || only_new)
        })
        .collect()
}

/// One row for a change shared by entries, rows for what stays with a single entry.
fn rows_of(
    area: &str,
    changed: Vec<(String, Vec<FieldDiff>)>,
    name_of: &dyn Fn(&str) -> String,
) -> Vec<Row> {
    type Sig = (String, Option<Leaf>, Option<Leaf>);
    let sig = |d: &FieldDiff| -> Sig { (d.path.clone(), d.old.clone(), d.new.clone()) };
    let mut shared: BTreeMap<Sig, (Kind, Vec<usize>)> = BTreeMap::new();
    for (i, (_, diffs)) in changed.iter().enumerate() {
        for d in diffs {
            shared.entry(sig(d)).or_insert_with(|| (d.kind, Vec::new())).1.push(i);
        }
    }
    shared.retain(|_, (_, entries)| entries.len() >= SHARED_FROM);
    let mut rows = Vec::new();
    for ((path, old, new), (kind, entries)) in &shared {
        let keys: Vec<String> = entries.iter().map(|&i| changed[i].0.clone()).collect();
        rows.push(Row {
            area: area.to_string(),
            fate: Fate::Changed,
            id: keys[0].clone(),
            name: name_of(&keys[0]),
            count: keys.len(),
            examples: keys.iter().take(EXAMPLES).cloned().collect(),
            changes: vec![FieldDiff { path: path.clone(), old: old.clone(), new: new.clone(), kind: *kind }],
            reader_note: false,
        });
    }
    for (key, diffs) in changed {
        let rest: Vec<FieldDiff> = diffs.into_iter().filter(|d| !shared.contains_key(&sig(d))).collect();
        if !rest.is_empty() {
            rows.push(Row {
                area: area.to_string(),
                fate: Fate::Changed,
                name: name_of(&key),
                examples: vec![key.clone()],
                id: key,
                count: 1,
                changes: rest,
                reader_note: false,
            });
        }
    }
    rows
}

/// The name of the warning for fields that the readers read and the archive no longer holds.
pub const FIELDS_GONE: &str = "form_feld_weg";

/// How many fields of the shapes are gone: those of a struct that vanished and those a struct lost.
fn fields_gone(old: &Area, new: &Area) -> i64 {
    let gone: usize = old
        .entries
        .iter()
        .map(|(key, fields)| match new.entries.get(key) {
            None => fields.len().max(1),
            Some(now) => fields.keys().filter(|field| !now.contains_key(*field)).count(),
        })
        .sum();
    i64::try_from(gone).unwrap_or(i64::MAX)
}

fn compare_area(cmp: &mut Comparison, name: &str, old: Option<&Area>, new: Option<&Area>, reader_changed: bool) {
    let empty = Area::default();
    let (old, new) = (old.unwrap_or(&empty), new.unwrap_or(&empty));
    let mut count = AreaCount { area: name.to_string(), old: old.entries.len(), new: new.entries.len(), ..AreaCount::default() };
    if reader_changed && (old.entries.is_empty() || new.entries.is_empty()) {
        cmp.counts.push(count);
        return;
    }
    if new.entries.is_empty() && !old.entries.is_empty() {
        let (old_n, class) = (i64::try_from(old.entries.len()).unwrap_or(i64::MAX), Class::Breaks);
        cmp.warnings.push(Warning { name: format!("bereich:{name}"), old: old_n, new: 0, class });
    }
    if name.starts_with(COUNT_ONLY) {
        // A text area holds a checksum per key: it is counted, not listed (a patch rewrites thousands of texts).
        count.removed = old.entries.keys().filter(|key| !new.entries.contains_key(*key)).count();
        for (key, entry) in &new.entries {
            match old.entries.get(key) {
                None => count.added += 1,
                Some(before) if before != entry => count.changed += 1,
                Some(_) => {}
            }
        }
        count.unchanged = new.entries.len().saturating_sub(count.added + count.changed);
        cmp.counts.push(count);
        return;
    }
    for (fate, from, other) in [(Fate::New, new, old), (Fate::Gone, old, new)] {
        for (key, entry) in &from.entries {
            if other.entries.contains_key(key) {
                continue;
            }
            match fate {
                Fate::New => count.added += 1,
                _ => count.removed += 1,
            }
            cmp.rows.push(Row {
                area: name.to_string(),
                fate,
                id: key.clone(),
                name: entry_name(key, entry),
                count: 1,
                examples: vec![key.clone()],
                changes: Vec::new(),
                reader_note: false,
            });
        }
    }
    if name == stands::AREA_FORMS {
        let gone = fields_gone(old, new);
        if gone > 0 {
            cmp.warnings.push(Warning { name: FIELDS_GONE.to_string(), old: 0, new: gone, class: Class::Breaks });
        }
    }
    let shapes = reader_changed.then(|| (shapes_of(old), shapes_of(new)));
    let common = shapes.as_ref().map(|(old, new)| Common { old, new });
    let mut changed: Vec<(String, Vec<FieldDiff>)> = Vec::new();
    let mut notes: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut touched = 0usize;
    for (key, old_entry) in &old.entries {
        let Some(new_entry) = new.entries.get(key) else { continue };
        if old_entry == new_entry {
            continue;
        }
        let mut found = Vec::new();
        let diffs = diff_entry(old_entry, new_entry, common.as_ref(), &mut found);
        if !diffs.is_empty() || !found.is_empty() {
            touched += 1;
        }
        found.into_iter().for_each(|note| notes.entry(note).or_default().push(key.clone()));
        if !diffs.is_empty() {
            changed.push((key.clone(), diffs));
        }
    }
    count.changed = touched;
    count.unchanged = new.entries.keys().filter(|k| old.entries.contains_key(*k)).count().saturating_sub(touched);
    let name_of = |key: &str| {
        new.entries.get(key).or_else(|| old.entries.get(key)).map_or_else(|| key.to_string(), |entry| entry_name(key, entry))
    };
    cmp.rows.extend(rows_of(name, changed, &name_of));
    for (path, mut keys) in notes {
        keys.dedup();
        cmp.rows.push(Row {
            area: name.to_string(),
            fate: Fate::Changed,
            id: keys[0].clone(),
            name: name_of(&keys[0]),
            count: keys.len(),
            examples: keys.iter().take(EXAMPLES).cloned().collect(),
            changes: vec![FieldDiff { path, old: None, new: None, kind: Kind::Shape }],
            reader_note: true,
        });
    }
    cmp.counts.push(count);
}

fn scale_of(stand: &Stand, area: &str) -> Option<Scale> {
    let area = stand.areas.get(area)?;
    let ceiling = area.entries.values().filter_map(|entry| match entry.get("") {
        Some(Leaf::Int(n)) => Some(*n),
        _ => None,
    });
    Some(Scale { levels: area.entries.len(), ceiling: ceiling.max() })
}

/// The probe counters that grew; a counter one stand lacks is not compared.
fn warnings_of(old: &Stand, new: &Stand) -> Vec<Warning> {
    new.probe
        .iter()
        .filter_map(|(name, now)| {
            let before = *old.probe.get(name)?;
            (*now > before).then(|| Warning {
                name: name.clone(),
                old: before,
                new: *now,
                class: if name == stands::PROBE_FAILED { Class::Breaks } else { Class::LooksDifferent },
            })
        })
        .collect()
}

/// What `new` changed against `old`.
pub fn compare(old: &Stand, new: &Stand) -> Comparison {
    let reader_changed = old.reader != new.reader;
    let mut cmp = Comparison { from: Meta::of(old), to: Meta::of(new), reader_changed, ..Comparison::default() };
    let names: BTreeSet<&String> = old.areas.keys().chain(new.areas.keys()).collect();
    for name in names {
        compare_area(&mut cmp, name, old.areas.get(name), new.areas.get(name), reader_changed);
    }
    cmp.warnings.extend(warnings_of(old, new));
    for area in SCALE_AREAS {
        let (before, now) = (scale_of(old, area), scale_of(new, area));
        if before.is_some() || now.is_some() {
            cmp.scales.push(ScaleRow { area: area.to_string(), old: before, new: now });
        }
    }
    let fate_rank = |fate: Fate| match fate {
        Fate::Changed => 0,
        Fate::New => 1,
        Fate::Gone => 2,
    };
    cmp.rows.sort_by(|a, b| {
        a.area
            .cmp(&b.area)
            .then(fate_rank(a.fate).cmp(&fate_rank(b.fate)))
            .then(b.count.cmp(&a.count))
            .then(b.kind().weight().cmp(&a.kind().weight()))
            .then_with(|| a.id.cmp(&b.id))
    });
    cmp
}

// ------------------------------------------------------------------ The file in the form of erkul

fn leaf_field(map: &mut Map, key: &str, leaf: Option<&Leaf>) {
    if let Some(leaf) = leaf {
        map.push((key.into(), leaf.to_value()));
    }
}

fn row_value(row: &Row) -> Value {
    let mut item: Map = vec![("id".into(), Value::str(&row.id)), ("name".into(), Value::str(&row.name))];
    if row.fate == Fate::Changed {
        let changes: Vec<Value> = row
            .changes
            .iter()
            .map(|change| {
                let mut fields: Map = vec![("path".into(), Value::str(&change.path))];
                leaf_field(&mut fields, "oldValue", change.old.as_ref());
                leaf_field(&mut fields, "newValue", change.new.as_ref());
                fields.push(("art".into(), Value::str(change.kind.key())));
                Value::Object(fields)
            })
            .collect();
        item.push(("changes".into(), Value::Array(changes)));
        item.push(("anzahl".into(), Value::int(i64::try_from(row.count).unwrap_or(i64::MAX))));
        let examples = row.examples.iter().map(Value::str).collect();
        item.push(("beispiele".into(), Value::Array(examples)));
        if row.reader_note {
            item.push(("hinweis".into(), Value::str("leser_geaendert")));
        }
    } else {
        item.push(("changes".into(), Value::Array(Vec::new())));
    }
    Value::Object(item)
}

fn count_value(n: usize) -> Value {
    Value::int(i64::try_from(n).unwrap_or(i64::MAX))
}

fn scale_value(scale: Option<&Scale>) -> Value {
    scale.map_or(Value::Null, |s| {
        Value::Object(vec![
            ("stufen".into(), count_value(s.levels)),
            ("decke".into(), s.ceiling.map_or(Value::Null, Value::int)),
        ])
    })
}

/// The comparison in the form of erkul's patches (`categories[]` with `kind`, `added`, `removed`, `modified[]`),
/// with what only an own comparison has (`anzahl`, `beispiele`, `kennzahlen`, `warnungen`) as further fields,
/// which the reader of erkul's files passes over.
pub fn to_patch(cmp: &Comparison) -> Value {
    let (added, removed, modified) = cmp.summary();
    let categories: Vec<Value> = cmp
        .counts
        .iter()
        .filter(|count| cmp.rows.iter().any(|r| r.area == count.area))
        .map(|count| {
            let of = |fate: Fate| -> Vec<Value> {
                cmp.rows.iter().filter(|r| r.area == count.area && r.fate == fate).map(row_value).collect()
            };
            Value::Object(vec![
                ("kind".into(), Value::str(&count.area)),
                ("unchanged".into(), count_value(count.unchanged)),
                ("added".into(), Value::Array(of(Fate::New))),
                ("removed".into(), Value::Array(of(Fate::Gone))),
                ("modified".into(), Value::Array(of(Fate::Changed))),
            ])
        })
        .collect();
    let counts: Vec<Value> = cmp
        .counts
        .iter()
        .map(|c| {
            Value::Object(vec![
                ("bereich".into(), Value::str(&c.area)),
                ("alt".into(), count_value(c.old)),
                ("neu".into(), count_value(c.new)),
                ("hinzu".into(), count_value(c.added)),
                ("weg".into(), count_value(c.removed)),
                ("geaendert".into(), count_value(c.changed)),
            ])
        })
        .collect();
    let scales: Vec<Value> = cmp
        .scales
        .iter()
        .map(|s| {
            Value::Object(vec![
                ("bereich".into(), Value::str(&s.area)),
                ("alt".into(), scale_value(s.old.as_ref())),
                ("neu".into(), scale_value(s.new.as_ref())),
            ])
        })
        .collect();
    let warnings: Vec<Value> = cmp
        .warnings
        .iter()
        .map(|w| {
            let class = match w.class {
                Class::Breaks => "bricht",
                Class::LooksDifferent => "sieht_anders_aus",
                Class::NewOrChanged => "neu_oder_geaendert",
            };
            Value::Object(vec![
                ("name".into(), Value::str(&w.name)),
                ("alt".into(), Value::int(w.old)),
                ("neu".into(), Value::int(w.new)),
                ("klasse".into(), Value::str(class)),
            ])
        })
        .collect();
    Value::Object(vec![
        (
            "summary".into(),
            Value::Object(vec![
                ("added".into(), count_value(added)),
                ("removed".into(), count_value(removed)),
                ("modified".into(), count_value(modified)),
            ]),
        ),
        ("version".into(), Value::str(&cmp.to.build)),
        ("von".into(), Value::str(&cmp.from.build)),
        ("kanal".into(), Value::str(&cmp.to.channel)),
        ("datum".into(), Value::str(&cmp.to.date)),
        ("quelle".into(), Value::str(SOURCE)),
        ("leser".into(), Value::str(&cmp.to.reader)),
        ("leser_geaendert".into(), Value::Bool(cmp.reader_changed)),
        ("vertraulich".into(), Value::Bool(cmp.to.confidential)),
        ("nda".into(), Value::Bool(cmp.to.under_agreement)),
        ("categories".into(), Value::Array(categories)),
        ("kennzahlen".into(), Value::Array(counts)),
        ("skalen".into(), Value::Array(scales)),
        ("warnungen".into(), Value::Array(warnings)),
    ])
}

/// Why the file was not written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WriteError {
    /// The program is not built for the channel.
    ChannelNotBuiltIn,
    Write(String),
}

/// Keeps the comparison as `Patches/eigen-<build>.json` where it says anything (a change, a warning, a scale that
/// changed). The file name; `None` for a comparison that says nothing.
///
/// # Errors
///
/// [`WriteError::ChannelNotBuiltIn`] for a channel the program is not built for, [`WriteError::Write`] when the
/// file cannot be written.
pub fn write_patch(paths: &Paths, cmp: &Comparison) -> Result<Option<String>, WriteError> {
    if !stands::built_in(&cmp.to.channel) {
        return Err(WriteError::ChannelNotBuiltIn);
    }
    if cmp.is_empty() {
        return Ok(None);
    }
    let target = crate::patch_changes::own_file(paths, &cmp.to.build);
    let text = crate::patch_changes::compact(&to_patch(cmp));
    jsonio::write_text_atomic(&target, &text, jsonio::Newline::Keep).map_err(|e| WriteError::Write(e.to_string()))?;
    Ok(Some(target))
}

#[cfg(test)]
mod tests;
