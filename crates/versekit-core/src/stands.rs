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

//! The stand of a game build: what the own readers made of it, kept flat and packed, so that the next build can be
//! compared with it (`stand_diff`) after the launcher has replaced the archive.
//!
//! A stand is a set of areas (one per list a reader makes); an area holds entries by key, an entry its values as
//! `path → value`. Numbers are kept as their bits, so that two readings are equal exactly when they are the same
//! number.
//!
//! ⚠ Confidential channels. A stand holds game content. The channels under an agreement
//! ([`paths::UNDER_AGREEMENT`]; PTU is public) are marked `vertraulich`, and a channel the program is not built for ([`paths::CHANNELS`]) is neither saved, nor listed,
//! nor loaded: the user build never holds a stand of a channel under an agreement. The marks are worked out from
//! the channel, never taken from the file.

use std::collections::BTreeMap;

use crate::jsonio::{self, Map, Value};
use crate::paths::{self, Paths};

/// The subfolder of the stands.
pub const FOLDER: &str = "Staende";
/// The layout of the file; a stand of another layout is not read.
pub const FORMAT: i64 = 1;
/// How many stands of a channel are kept.
pub const KEEP_PER_CHANNEL: usize = 4;
/// What the whole folder may take, in bytes.
pub const MAX_BYTES: u64 = 64 * 1024 * 1024;
/// The ending of a stand file; its content is a zlib stream of JSON.
pub const EXTENSION: &str = ".stand";
/// The reader of a stand made from the kept caches.
pub const READER_CACHES: &str = "zwischenspeicher";
/// The most a stand may unpack to.
const LARGEST_UNPACKED: usize = 1 << 29;

/// A cache file whose content goes into a stand, and the name its areas begin with.
pub struct CacheFile {
    pub file: &'static str,
    pub area: &'static str,
    /// The key under which the lists lie, where they do not lie at the top.
    pub wrapper: Option<&'static str>,
}

/// Every cache a stand is made from. The one place that lists the areas: a new area is added here.
pub const CACHES: [CacheFile; 6] = [
    CacheFile { file: "katalog-spieldaten.json", area: "katalog", wrapper: None },
    CacheFile { file: "werte-spieldaten.json", area: "werte", wrapper: Some("values") },
    CacheFile { file: "spiel-schiffe.json", area: "schiffe", wrapper: None },
    CacheFile { file: "spiel-teile.json", area: "teile", wrapper: None },
    CacheFile { file: "auftragsruf.json", area: "auftragsruf", wrapper: None },
    CacheFile { file: "rufstufen.json", area: "rufstufen", wrapper: None },
];

/// What describes the file, not the game: left out of the areas.
const HEADER_KEYS: [&str; 10] =
    ["format", "quelle", "stamp", "version", "spielversion", "build", "archiv", "archiv_stand", "datenfehler", "angaben_quelle"];

/// The fields that name an entry of a list, in the order they are tried.
const IDENTITY: [&str; 9] = ["key", "klasse", "class", "className", "tag", "guid", "id", "titel_key", "name"];

/// The area of the shapes: for every struct of the DataForge that the readers read, its fields with their types.
/// A field that goes, a type that changes or a struct that vanishes shows in the comparison like any other change.
pub const AREA_FORMS: &str = "form.felder";

/// The counters of the reading probe: gaps that can grow without a reader failing.
pub const PROBE_FAILED: &str = "failed";
pub const PROBE_PLACEHOLDERS: &str = "platzhalter";
pub const PROBE_EMPTY_TEXTS: &str = "leere_texte";
pub const PROBE_NO_VALUE: &str = "ohne_wert";

/// One value of an entry.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Leaf {
    Null,
    Bool(bool),
    Int(i64),
    /// A number with a fraction, as its bits.
    Num(u64),
    Text(String),
}

impl Leaf {
    pub fn number(x: f64) -> Leaf {
        Leaf::Num(x.to_bits())
    }

    /// The value of a JSON scalar; an empty list or object is the text `[]` or `{}`, a filled one is no leaf.
    pub fn from_value(value: &Value) -> Option<Leaf> {
        Some(match value {
            Value::Null => Leaf::Null,
            Value::Bool(b) => Leaf::Bool(*b),
            Value::Int(text) => text.parse::<i64>().map_or_else(|_| Leaf::Text(text.clone()), Leaf::Int),
            Value::Float(x) => Leaf::number(*x),
            Value::Str(text) => Leaf::Text(text.clone()),
            Value::Array(items) if items.is_empty() => Leaf::Text("[]".into()),
            Value::Object(map) if map.is_empty() => Leaf::Text("{}".into()),
            _ => return None,
        })
    }

    pub fn to_value(&self) -> Value {
        match self {
            Leaf::Null => Value::Null,
            Leaf::Bool(b) => Value::Bool(*b),
            Leaf::Int(n) => Value::int(*n),
            Leaf::Num(bits) => Value::Float(f64::from_bits(*bits)),
            Leaf::Text(text) => Value::str(text),
        }
    }

    /// The value as it is shown.
    pub fn show(&self) -> String {
        match self {
            Leaf::Null => "null".into(),
            Leaf::Bool(b) => b.to_string(),
            Leaf::Int(n) => n.to_string(),
            Leaf::Num(bits) => f64::from_bits(*bits).to_string(),
            Leaf::Text(text) => text.clone(),
        }
    }

    /// A number or a truth value: a change of it is a change of the value, not of the form.
    pub fn is_countable(&self) -> bool {
        matches!(self, Leaf::Bool(_) | Leaf::Int(_) | Leaf::Num(_))
    }

    /// The same number, whether a reader wrote it with or without a fraction (`60` and `60.0`), compared exactly.
    pub fn same_as(&self, other: &Leaf) -> bool {
        match (self, other) {
            (Leaf::Int(n), Leaf::Num(bits)) | (Leaf::Num(bits), Leaf::Int(n)) => {
                let x = f64::from_bits(*bits);
                x.fract() == 0.0 && x.abs() < 9.0e15 && (x as i64) == *n
            }
            _ => self == other,
        }
    }
}

/// The values of one entry: path → value.
pub type Entry = BTreeMap<String, Leaf>;

/// A list a reader makes: entries by key.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Area {
    pub entries: BTreeMap<String, Entry>,
}

fn join(path: &str, key: &str) -> String {
    if path.is_empty() { key.to_string() } else { format!("{path}.{key}") }
}

/// The values of a JSON value, flat: an object adds `.key`, a list `[i]`.
pub fn flatten(value: &Value, path: &str, into: &mut Entry) {
    match value {
        Value::Object(map) if !map.is_empty() => map.iter().for_each(|(key, item)| flatten(item, &join(path, key), into)),
        Value::Array(items) if !items.is_empty() => {
            items.iter().enumerate().for_each(|(i, item)| flatten(item, &format!("{path}[{i}]"), into));
        }
        other => {
            if let Some(leaf) = Leaf::from_value(other) {
                into.insert(path.to_string(), leaf);
            }
        }
    }
}

/// The key of an item of a list: the first of [`IDENTITY`] it has.
fn identity(item: &Value) -> Option<String> {
    IDENTITY.iter().find_map(|field| match item.get(field) {
        Some(Value::Str(text)) if !text.is_empty() => Some(text.clone()),
        Some(Value::Int(number)) => Some(number.clone()),
        _ => None,
    })
}

/// The entries of a list or an object: a list by the key of its items (a repeated key gets `#2`, `#3` …), else by
/// place; an object by its keys.
fn entries_of(value: &Value) -> BTreeMap<String, Entry> {
    let mut out = BTreeMap::new();
    match value {
        Value::Object(map) => {
            for (key, item) in map {
                let mut entry = Entry::new();
                flatten(item, "", &mut entry);
                out.insert(key.clone(), entry);
            }
        }
        Value::Array(items) => {
            let mut seen: BTreeMap<String, usize> = BTreeMap::new();
            for (place, item) in items.iter().enumerate() {
                let base = identity(item).unwrap_or_else(|| format!("[{place}]"));
                let times = seen.entry(base.clone()).or_default();
                *times += 1;
                let key = if *times == 1 { base } else { format!("{base}#{times}") };
                let mut entry = Entry::new();
                flatten(item, "", &mut entry);
                out.insert(key, entry);
            }
        }
        _ => {}
    }
    out
}

/// What a stand leaves out of an entry, by area and field: what only scmdb fills in (a stand says "from the
/// archive"; the field would come and go with the network, and read like a change of the game), and what the device
/// works out itself (the patch a blueprint was seen with: `patch_history`).
pub const SKIPPED: [(&str, &str); 4] = [
    ("katalog.missionen", "sicher"),
    ("katalog.missionen", "chance"),
    ("katalog.missionen", "uec"),
    ("katalog.bauplaene", "seit"),
];

/// Takes the fields of [`SKIPPED`] out of the entries of `area`.
fn drop_skipped(area: &str, entries: &mut BTreeMap<String, Entry>) {
    for (_, field) in SKIPPED.iter().filter(|(name, _)| *name == area) {
        for entry in entries.values_mut() {
            entry.retain(|path, _| {
                let below = path.strip_prefix(field).is_some_and(|rest| rest.starts_with(['.', '[']));
                path != field && !below
            });
        }
    }
}

/// Adds the areas of the content of one cache file (`root`) to `into`: every list or object under the top (or the
/// wrapper) that is no header. The one place the content of a cache and of a reader's result become areas, so that
/// the stand of the kept caches and the stand of a reading are made the same way.
pub fn add_areas(cache: &CacheFile, root: &Value, into: &mut BTreeMap<String, Area>) {
    let base = match cache.wrapper {
        Some(wrapper) => root.get(wrapper),
        None => Some(root),
    };
    let Some(Value::Object(map)) = base else { return };
    for (key, value) in map {
        if HEADER_KEYS.contains(&key.as_str()) || !matches!(value, Value::Array(_) | Value::Object(_)) {
            continue;
        }
        let name = format!("{}.{key}", cache.area);
        let mut entries = entries_of(value);
        drop_skipped(&name, &mut entries);
        into.insert(name, Area { entries });
    }
}

/// A name the game fills in later (`~mission(Title)`), left in the text because the reader could not.
pub fn is_placeholder(text: &str) -> bool {
    let text = text.trim();
    text.starts_with('~') && text.ends_with(')') && text.contains('(')
}

/// The reading probe of the areas: the counters that may grow without any reader failing.
pub fn probe_of(areas: &BTreeMap<String, Area>) -> BTreeMap<String, i64> {
    let mut counter: BTreeMap<String, i64> = BTreeMap::new();
    for (name, area) in areas {
        if name.rsplit('.').next() == Some("failed") {
            *counter.entry(PROBE_FAILED.into()).or_default() += i64::try_from(area.entries.len()).unwrap_or(i64::MAX);
        }
        for leaf in area.entries.values().flat_map(BTreeMap::values) {
            let key = match leaf {
                Leaf::Null => PROBE_NO_VALUE,
                Leaf::Text(text) if text.is_empty() => PROBE_EMPTY_TEXTS,
                Leaf::Text(text) if is_placeholder(text) => PROBE_PLACEHOLDERS,
                _ => continue,
            };
            *counter.entry(key.into()).or_default() += 1;
        }
    }
    for key in [PROBE_FAILED, PROBE_PLACEHOLDERS, PROBE_EMPTY_TEXTS, PROBE_NO_VALUE] {
        counter.entry(key.into()).or_default();
    }
    counter
}

/// Today as `YYYY-MM-DD`, the date of a stand.
pub fn today() -> String {
    let (year, month, day, ..) = jsonio::local_time();
    format!("{year:04}-{month:02}-{day:02}")
}

/// The channel is one the program is built for: the user build knows LIVE, HOTFIX and PTU, the test build all.
pub fn built_in(channel: &str) -> bool {
    paths::CHANNELS.iter().any(|c| c.eq_ignore_ascii_case(channel))
}

/// What the channel holds is confidential: only a channel under an agreement (EPTU, TECH-PREVIEW). PTU is public.
pub fn is_confidential(channel: &str) -> bool {
    is_under_agreement(channel)
}

/// The channel stands under an agreement (a non-disclosure agreement).
pub fn is_under_agreement(channel: &str) -> bool {
    paths::UNDER_AGREEMENT.iter().any(|c| c.eq_ignore_ascii_case(channel))
}

/// What the readers made of one build.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stand {
    pub channel: String,
    pub build: String,
    /// `YYYY-MM-DD`.
    pub date: String,
    /// What says which archive it was read from (size and time of the file), as text.
    pub archive_stamp: String,
    /// What made the stand: [`READER_CACHES`] or the version of the reader.
    pub reader: String,
    pub confidential: bool,
    pub under_agreement: bool,
    pub probe: BTreeMap<String, i64>,
    pub areas: BTreeMap<String, Area>,
}

impl Stand {
    /// An empty stand of `channel`; the marks follow from the channel.
    pub fn new(channel: &str, build: &str, date: &str, reader: &str) -> Stand {
        Stand {
            channel: channel.to_string(),
            build: build.to_string(),
            date: date.to_string(),
            archive_stamp: String::new(),
            reader: reader.to_string(),
            confidential: is_confidential(channel),
            under_agreement: is_under_agreement(channel),
            probe: BTreeMap::new(),
            areas: BTreeMap::new(),
        }
    }

    /// The stand of what the cache files hold now (the base before the first stand of the readers). `None`
    /// when none of them can be read.
    pub fn from_cache_files(files: &[(&CacheFile, String)], channel: &str, build: &str, date: &str) -> Option<Stand> {
        let mut stand = Stand::new(channel, build, date, READER_CACHES);
        let mut read = false;
        for (cache, path) in files {
            let Ok(root) = jsonio::read_json(path) else { continue };
            read = true;
            add_areas(cache, &root, &mut stand.areas);
            if let Some(stamp) = root.get("quelle").and_then(|source| source.get("archiv_stand")) {
                stand.archive_stamp = jsonio::dumps(stamp, None, true);
            }
        }
        stand.probe = probe_of(&stand.areas);
        read.then_some(stand)
    }

    /// The stand of the caches of the main data folder.
    pub fn from_caches(paths: &Paths, channel: &str, build: &str, date: &str) -> Option<Stand> {
        let files: Vec<(&CacheFile, String)> = CACHES.iter().map(|c| (c, paths.app_file(c.file))).collect();
        Stand::from_cache_files(&files, channel, build, date)
    }

    /// The program is built for the channel of this stand.
    pub fn allowed_here(&self) -> bool {
        built_in(&self.channel)
    }

    /// How many entries all areas hold.
    pub fn entry_count(&self) -> usize {
        self.areas.values().map(|a| a.entries.len()).sum()
    }

    /// The stand as JSON.
    pub fn to_value(&self) -> Value {
        let areas: Map = self
            .areas
            .iter()
            .map(|(name, area)| {
                let entries: Map = area
                    .entries
                    .iter()
                    .map(|(key, entry)| {
                        let leaves: Map = entry.iter().map(|(path, leaf)| (path.clone(), leaf.to_value())).collect();
                        (key.clone(), Value::Object(leaves))
                    })
                    .collect();
                (name.clone(), Value::Object(entries))
            })
            .collect();
        let probe: Map = self.probe.iter().map(|(name, n)| (name.clone(), Value::int(*n))).collect();
        Value::Object(vec![
            ("format".into(), Value::int(FORMAT)),
            ("kanal".into(), Value::str(&self.channel)),
            ("build".into(), Value::str(&self.build)),
            ("datum".into(), Value::str(&self.date)),
            ("archiv_stand".into(), Value::str(&self.archive_stamp)),
            ("leser".into(), Value::str(&self.reader)),
            ("vertraulich".into(), Value::Bool(self.confidential)),
            ("nda".into(), Value::Bool(self.under_agreement)),
            ("leseprobe".into(), Value::Object(probe)),
            ("bereiche".into(), Value::Object(areas)),
        ])
    }

    /// A stand from its JSON; `None` for another layout or a file that is none. The marks are worked out from the
    /// channel, what the file says of them does not count.
    pub fn from_value(value: &Value) -> Option<Stand> {
        let text = |key: &str| value.get(key).and_then(Value::as_str).map(str::to_string);
        if !matches!(value.get("format"), Some(Value::Int(n)) if n.parse::<i64>() == Ok(FORMAT)) {
            return None;
        }
        let mut stand = Stand::new(&text("kanal")?, &text("build")?, &text("datum").unwrap_or_default(), &text("leser")?);
        stand.archive_stamp = text("archiv_stand").unwrap_or_default();
        if let Some(Value::Object(probe)) = value.get("leseprobe") {
            for (name, n) in probe {
                if let Value::Int(n) = n {
                    stand.probe.insert(name.clone(), n.parse().unwrap_or(0));
                }
            }
        }
        let Some(Value::Object(areas)) = value.get("bereiche") else { return None };
        for (name, entries) in areas {
            let mut area = Area::default();
            for (key, leaves) in entries.as_object()? {
                let mut entry = Entry::new();
                for (path, leaf) in leaves.as_object()? {
                    entry.insert(path.clone(), Leaf::from_value(leaf)?);
                }
                area.entries.insert(key.clone(), entry);
            }
            stand.areas.insert(name.clone(), area);
        }
        Some(stand)
    }
}

/// The folder of the stands: `Staende` in the data folder, flat in a data folder given by the home variable (`envvars::ENV_HOME`).
pub fn folder(paths: &Paths) -> String {
    let base = paths.app_folder();
    if paths.host.var_set(crate::envvars::ENV_HOME).is_some() {
        return base;
    }
    let target = paths.host.flavor.join(&base, FOLDER);
    if jsonio::make_dirs(&target).is_err() {
        return base;
    }
    target
}

/// The file name of a stand: `<channel>_<build>.stand`.
pub fn file_name(channel: &str, build: &str) -> String {
    format!("{}_{}{EXTENSION}", crate::patch_changes::safe_name(channel), crate::patch_changes::safe_name(build))
}

fn path_of(paths: &Paths, channel: &str, build: &str) -> String {
    paths.host.flavor.join(&folder(paths), &file_name(channel, build))
}

/// Why a stand was not saved.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SaveError {
    /// The program is not built for the channel: the user build never keeps a stand of EPTU or Tech-Preview.
    ChannelNotBuiltIn,
    Write(String),
}

/// Keeps `stand` and drops the old ones beyond [`KEEP_PER_CHANNEL`] and [`MAX_BYTES`].
///
/// # Errors
///
/// [`SaveError::ChannelNotBuiltIn`] for a channel the program is not built for; [`SaveError::Write`] when the
/// file cannot be written.
pub fn save(paths: &Paths, stand: &Stand) -> Result<(), SaveError> {
    if !stand.allowed_here() {
        return Err(SaveError::ChannelNotBuiltIn);
    }
    let text = crate::patch_changes::compact(&stand.to_value());
    let packed = miniz_oxide::deflate::compress_to_vec_zlib(text.as_bytes(), 6);
    let target = path_of(paths, &stand.channel, &stand.build);
    let _guard = jsonio::lock_saves();
    let temp = jsonio::temp_name(&target);
    let written = jsonio::write_new(&temp, &packed).and_then(|()| jsonio::replace_file(&temp, &target));
    if let Err(error) = written {
        let _ = jsonio::remove_file(&temp);
        return Err(SaveError::Write(error.to_string()));
    }
    prune(paths);
    Ok(())
}

/// The stand of `channel` and `build`; `None` when there is none, it is of another layout, or the program is not
/// built for the channel.
pub fn load(paths: &Paths, channel: &str, build: &str) -> Option<Stand> {
    if !built_in(channel) {
        return None;
    }
    let packed = std::fs::read(path_of(paths, channel, build)).ok()?;
    let raw = miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(&packed, LARGEST_UNPACKED).ok()?;
    let stand = Stand::from_value(&jsonio::loads(&String::from_utf8(raw).ok()?).ok()?)?;
    stand.allowed_here().then_some(stand)
}

/// One stand file kept.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Info {
    pub channel: String,
    pub build: String,
    pub name: String,
    pub bytes: u64,
}

/// The channel and the build of a file name.
fn parse_name(name: &str) -> Option<(String, String)> {
    let (channel, build) = name.strip_suffix(EXTENSION)?.split_once('_')?;
    (!channel.is_empty() && !build.is_empty()).then(|| (channel.to_string(), build.to_string()))
}

/// The stands kept, channel by channel (the order of [`paths::CHANNELS`]), the newest of a channel first.
/// A file of a channel the program is not built for is not listed.
pub fn list(paths: &Paths) -> Vec<Info> {
    let Ok(entries) = std::fs::read_dir(folder(paths)) else { return Vec::new() };
    let mut found: Vec<Info> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().to_str()?.to_string();
            let (channel, build) = parse_name(&name)?;
            built_in(&channel).then(|| Info { channel, build, name, bytes: entry.metadata().map_or(0, |m| m.len()) })
        })
        .collect();
    let place = |channel: &str| paths::CHANNELS.iter().position(|c| c.eq_ignore_ascii_case(channel)).unwrap_or(usize::MAX);
    found.sort_by(|a, b| {
        place(&a.channel)
            .cmp(&place(&b.channel))
            .then_with(|| crate::patch_history::rank(&b.build).cmp(&crate::patch_history::rank(&a.build)))
            .then_with(|| b.name.cmp(&a.name))
    });
    found
}

/// Drops the old stands: of each channel only the newest [`KEEP_PER_CHANNEL`], and of all of them as many as
/// [`MAX_BYTES`] allows (the oldest first, never the newest of a channel). The names dropped.
pub fn prune(paths: &Paths) -> Vec<String> {
    prune_with(paths, KEEP_PER_CHANNEL, MAX_BYTES)
}

/// [`prune`] with the limits given.
pub fn prune_with(paths: &Paths, keep: usize, max_bytes: u64) -> Vec<String> {
    let all = list(paths);
    let folder = folder(paths);
    let mut dropped = Vec::new();
    let mut remaining: Vec<Info> = Vec::new();
    let mut per_channel: BTreeMap<String, usize> = BTreeMap::new();
    for info in all {
        let count = per_channel.entry(info.channel.to_ascii_uppercase()).or_default();
        *count += 1;
        if *count > keep.max(1) {
            dropped.push(info);
        } else {
            remaining.push(info);
        }
    }
    let mut total: u64 = remaining.iter().map(|i| i.bytes).sum();
    while total > max_bytes {
        // The oldest that is not the newest of its channel: the last one of the list whose channel has another before it.
        let Some(at) = (0..remaining.len()).rev().find(|&i| {
            remaining[..i].iter().any(|before| before.channel.eq_ignore_ascii_case(&remaining[i].channel))
        }) else {
            break;
        };
        let info = remaining.remove(at);
        total -= info.bytes;
        dropped.push(info);
    }
    dropped
        .into_iter()
        .filter_map(|info| {
            let target = paths.host.flavor.join(&folder, &info.name);
            jsonio::remove_file(&target).is_ok().then_some(info.name)
        })
        .collect()
}

#[cfg(test)]
mod tests;
