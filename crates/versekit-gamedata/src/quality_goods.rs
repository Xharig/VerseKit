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

//! The goods that carry a quality, read from the game database of the own installation (`quality_goods.py`).
//!
//! Every good with a quality has a `CraftingQualityQuantizationRecord` (`Quantization_<stem>`). Its readable name
//! comes from the English `global.ini`, found in this order and only where it is unique:
//!
//! 1. a `ResourceType` that links to the record,
//! 2. a `ResourceType` of the same name (without `Ore`/`Raw`, without `_`),
//! 3. the text `items_commodities_<stem>`,
//! 4. a text `items_commodities_…` that holds every word of the stem (short words also as an abbreviation:
//!    `VLK` → `valakkar`), as the only one or as the start of a family (quality steps of the same good),
//! 5. a key with a slight typo, when exactly one lies that close.
//!
//! What cannot be named so stays out. A good that is refined (a `ResourceType` with a `refinedVersion`) counts in
//! SCU, everything else in pieces; a piece states its size in SCU where its hand item (`harvestable_…`) has
//! `microSCU`.
//!
//! The list is read once per state of the `Data.p4k` (size and time) and kept as [`FILE`] in the data folder.

use std::collections::{BTreeSet, HashMap};
use std::path::Path;

use p4kx_dataforge::{DataForge, Guid, RecordTree, StructValue, Value};
use versekit_core::jsonio::{self, Newline, Value as Json};
use versekit_core::text::{decode_ignore, py_splitlines, py_strip};

use crate::Error;
use crate::fields::link;

/// The file in the data folder.
pub use versekit_core::stock_data::QUALITY_GOODS_FILE as FILE;
const QUANT_TYPE: &str = "CraftingQualityQuantizationRecord";
/// How many characters of `Quantization_` are cut off the record name.
const QUANT_PREFIX_CHARS: usize = "Quantization_".len();
const COMMODITY_PREFIX: &str = "items_commodities_";
/// The layout of [`FILE`]; with another number the list is read again without a patch.
pub const FORMAT: i64 = 4;
/// Fewer goods mean the database was not understood: then nothing is kept.
pub const MINIMUM: usize = 20;

/// How a good is counted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Unit {
    Scu,
    Piece,
}

impl Unit {
    /// The word the file keeps: `scu` or `stueck`.
    pub const fn key(self) -> &'static str {
        match self {
            Unit::Scu => "scu",
            Unit::Piece => "stueck",
        }
    }
}

/// One good with a quality.
#[derive(Debug, Clone, PartialEq)]
pub struct Good {
    pub name: String,
    pub unit: Unit,
    /// SCU per piece, where the hand item states it.
    pub piece_scu: Option<f64>,
}

/// The texts of a `global.ini`: key in lower case (without what follows a `,`) → text; the first line of a key wins.
pub fn parse_ini(raw: &[u8]) -> HashMap<String, String> {
    texts_in_order(raw).into_iter().collect()
}

/// The texts of a `global.ini` in the order of the file, each key once.
fn texts_in_order(raw: &[u8]) -> Vec<(String, String)> {
    let text = decode_ignore(raw);
    let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for line in py_splitlines(text) {
        let Some((key, value)) = line.split_once('=') else { continue };
        let key = py_strip(key.split(',').next().unwrap_or_default()).to_lowercase();
        if seen.insert(key.clone()) {
            out.push((key, py_strip(value).to_string()));
        }
    }
    out
}

/// The record name without what comes up to its first `.`.
fn after_type(name: &str) -> &str {
    name.split_once('.').map_or(name, |(_, rest)| rest)
}

/// `Ore_Iron`, `Raw_Ice`, `RawSilicon` → `iron`, `ice`, `silicon`.
fn flat(name: &str) -> String {
    let mut s = name.to_lowercase().replace('_', "");
    for prefix in ["ore", "raw"] {
        if let Some(rest) = s.strip_prefix(prefix) {
            s = rest.to_string();
        }
    }
    s
}

/// The words of a stem in lower case: `QuasiTongue` → `quasi`, `tongue`; `VLKApex` → `vlk`, `apex`.
fn words(stem: &str) -> Vec<String> {
    let chars: Vec<char> = stem.chars().collect();
    let upper = |i: usize| chars.get(i).is_some_and(char::is_ascii_uppercase);
    let lower = |i: usize| chars.get(i).is_some_and(char::is_ascii_lowercase);
    let digit = |i: usize| chars.get(i).is_some_and(char::is_ascii_digit);
    let mut out = Vec::new();
    let mut at = 0;
    while at < chars.len() {
        // A run of capitals that ends before a capital with a small letter after it, or at the end.
        let mut run = 0;
        while upper(at + run) {
            run += 1;
        }
        let capitals = (1..=run).rev().find(|&len| {
            let next = at + len;
            next == chars.len() || (upper(next) && lower(next + 1))
        });
        let end = if let Some(len) = capitals {
            Some(at + len)
        } else if upper(at) && lower(at + 1) {
            let mut end = at + 1;
            while lower(end) {
                end += 1;
            }
            Some(end)
        } else if lower(at) || digit(at) {
            let small = lower(at);
            let mut end = at;
            while (small && lower(end)) || (!small && digit(end)) {
                end += 1;
            }
            Some(end)
        } else {
            None
        };
        match end {
            Some(end) => {
                out.push(chars[at..end].iter().collect::<String>().to_lowercase());
                at = end;
            }
            None => at += 1,
        }
    }
    out
}

/// Whether every letter of `short` stands in `word` in order, with the same start: `vlk` in `valakkar`.
fn in_order(short: &str, word: &str) -> bool {
    let (Some(a), Some(b)) = (short.chars().next(), word.chars().next()) else { return false };
    if a != b {
        return false;
    }
    let mut rest = word.chars();
    short.chars().all(|c| rest.any(|w| w == c))
}

/// Whether a word of the stem stands in the key: whole, or a short one as an abbreviation of a part of it.
fn word_hit(word: &str, key: &str) -> bool {
    if key.contains(word) {
        return true;
    }
    word.chars().count() <= 4 && key.split(|c: char| c == '_' || !c.is_alphanumeric()).any(|part| in_order(word, part))
}

/// The one key all others begin with.
fn family_root<'a>(keys: &[&'a str]) -> Option<&'a str> {
    keys.iter().copied().find(|key| keys.iter().all(|other| other.starts_with(key)))
}

/// The name found by the words of the stem, only where it is unique.
fn name_by_words<'t>(stem: &str, texts: &'t HashMap<String, String>, commodity_keys: &[&str]) -> Option<&'t str> {
    let words = words(stem);
    let hits: Vec<&str> = commodity_keys
        .iter()
        .copied()
        .filter(|key| !words.is_empty() && words.iter().all(|w| word_hit(w, &key[COMMODITY_PREFIX.len()..])))
        .collect();
    if let [only] = hits.as_slice() {
        return texts.get(*only).map(String::as_str);
    }
    if let Some(root) = family_root(&hits) {
        return texts.get(root).map(String::as_str);
    }
    // A slight typo in the key (`amoishi` / `amioshi`): only when exactly one key lies that close.
    let wanted = format!("{COMMODITY_PREFIX}{}", stem.to_lowercase());
    let mut close = commodity_keys.iter().filter(|key| crate::list::similarity(key.as_bytes(), wanted.as_bytes()) >= 0.9);
    match (close.next(), close.next()) {
        (Some(only), None) => texts.get(*only).map(String::as_str),
        _ => None,
    }
}

/// The links of a struct and everything inside it, in the order of its fields.
fn links_in(value: &StructValue<'_>, out: &mut BTreeSet<Guid>) {
    for field in value.fields() {
        links_of(field.value(), out);
    }
}

fn links_of(value: &Value<'_>, out: &mut BTreeSet<Guid>) {
    match value {
        Value::Struct(inner) => links_in(inner, out),
        Value::Array(items) => items.iter().for_each(|item| links_of(item, out)),
        other => out.extend(link(other)),
    }
}

/// Whether the field holds a link to a record, or anything else that is not empty.
fn is_set(value: Option<&Value<'_>>) -> bool {
    match value {
        None | Some(Value::Null) => false,
        Some(Value::String(text)) => !text.is_empty(),
        Some(Value::Reference(reference)) => reference.guid() != Guid::from_stored([0; 16]),
        Some(_) => true,
    }
}

/// The first `microSCU` the struct or anything inside it states as a whole number of zero or more, in the order of
/// its fields.
fn micro_scu(value: &StructValue<'_>) -> Option<i64> {
    for field in value.fields() {
        if field.name() == "microSCU"
            && let Some(number) = whole(field.value())
        {
            return Some(number);
        }
        let inner = match field.value() {
            Value::Struct(inner) => micro_scu(inner),
            Value::Array(items) => items.iter().find_map(|item| match item {
                Value::Struct(inner) => micro_scu(inner),
                _ => None,
            }),
            _ => None,
        };
        if inner.is_some() {
            return inner;
        }
    }
    None
}

fn whole(value: &Value<'_>) -> Option<i64> {
    let number = match *value {
        Value::Int8(v) => i64::from(v),
        Value::Int16(v) => i64::from(v),
        Value::Int32(v) => i64::from(v),
        Value::Int64(v) => v,
        Value::UInt8(v) => i64::from(v),
        Value::UInt16(v) => i64::from(v),
        Value::UInt32(v) => i64::from(v),
        Value::UInt64(v) => i64::try_from(v).ok()?,
        Value::Single(v) if v.is_finite() => f64::from(v).trunc() as i64,
        Value::Double(v) if v.is_finite() => v.trunc() as i64,
        _ => return None,
    };
    (number >= 0).then_some(number)
}

/// A `ResourceType`: its record name, the key of its name, the quantization records it links to, refined or not.
struct ResourceType {
    name: String,
    key: String,
    stems: BTreeSet<String>,
    refined: bool,
}

/// The goods with a quality the DataForge `forge` holds, named from `texts` (the lines of the English
/// `global.ini`), by name without regard to case.
pub fn extract(forge: &DataForge<'_>, raw_ini: &[u8]) -> Vec<Good> {
    let ordered = texts_in_order(raw_ini);
    let commodity_keys: Vec<&str> = ordered
        .iter()
        .map(|(key, _)| key.as_str())
        .filter(|key| key.starts_with(COMMODITY_PREFIX) && !key.ends_with("_desc"))
        .collect();
    let texts: HashMap<String, String> = ordered.iter().cloned().collect();
    let mut builder = p4kx_dataforge::TreeBuilder::new();
    let mut quant: HashMap<Guid, String> = HashMap::new();
    let mut resource_records = Vec::new();
    let mut harvestables: Vec<(String, Guid)> = Vec::new();
    let mut harvestable_at: HashMap<String, usize> = HashMap::new();
    for record in forge.records() {
        let (Ok(kind), Ok(name)) = (record.type_name(), record.name()) else { continue };
        match kind {
            QUANT_TYPE => {
                quant.insert(record.guid(), after_type(name).chars().skip(QUANT_PREFIX_CHARS).collect());
            }
            "ResourceType" => resource_records.push((after_type(name).to_string(), record)),
            "EntityClassDefinition" => {
                let short = after_type(name).to_lowercase();
                if short.starts_with("harvestable_") {
                    match harvestable_at.get(&short) {
                        Some(&at) => harvestables[at].1 = record.guid(),
                        None => {
                            harvestable_at.insert(short.clone(), harvestables.len());
                            harvestables.push((short, record.guid()));
                        }
                    }
                }
            }
            _ => {}
        }
    }
    let resource_types: Vec<ResourceType> = resource_records
        .into_iter()
        .filter_map(|(name, record)| {
            let tree: RecordTree<'_> = builder.build(&record).ok()?;
            let mut linked = BTreeSet::new();
            links_in(tree.root(), &mut linked);
            let key = match tree.field("displayName") {
                Some(Value::Locale(text) | Value::String(text)) => text.trim_start_matches('@').to_lowercase(),
                _ => String::new(),
            };
            Some(ResourceType {
                name,
                key,
                stems: linked.iter().filter_map(|guid| quant.get(guid).cloned()).collect(),
                refined: is_set(tree.field("refinedVersion")),
            })
        })
        .collect();
    let stems: BTreeSet<&String> = quant.values().collect();
    let mut found: BTreeSet<(String, Unit, Option<u64>)> = BTreeSet::new();
    let mut result = Vec::new();
    for stem in stems {
        if stem.to_uppercase() == "TEMPLATE" {
            continue;
        }
        let mut matches: Vec<&ResourceType> = resource_types.iter().filter(|rt| rt.stems.contains(stem)).collect();
        if matches.is_empty() {
            matches = resource_types.iter().filter(|rt| flat(&rt.name) == flat(stem)).collect();
        }
        let name = matches
            .iter()
            .find_map(|rt| texts.get(&rt.key).filter(|text| !text.is_empty() && !text.starts_with('@')))
            .map(String::as_str)
            .or_else(|| {
                texts.get(&format!("{COMMODITY_PREFIX}{}", stem.to_lowercase())).map(String::as_str).filter(|t| !t.is_empty())
            })
            .or_else(|| name_by_words(stem, &texts, &commodity_keys));
        let Some(name) = name.filter(|name| !name.is_empty()) else { continue };
        let scu = matches.iter().any(|rt| rt.refined);
        let piece_scu = if scu {
            None
        } else {
            piece_entity(stem, &harvestables)
                .and_then(|guid| forge.record_by_guid(guid))
                .and_then(|record| builder.build(&record).ok())
                .and_then(|tree| micro_scu(tree.root()))
                .filter(|micro| *micro != 0)
                .map(|micro| micro as f64 / 1e6)
        };
        let unit = if scu { Unit::Scu } else { Unit::Piece };
        if found.insert((name.to_string(), unit, piece_scu.map(f64::to_bits))) {
            result.push(Good { name: name.to_string(), unit, piece_scu });
        }
    }
    result.sort_by_cached_key(|good| good.name.to_lowercase());
    result
}

/// The hand item of a stem: `harvestable_…_<stem>` (also with `ore` after it), the shortest name, so not `…_cave`.
fn piece_entity(stem: &str, harvestables: &[(String, Guid)]) -> Option<Guid> {
    let s = stem.to_lowercase();
    let (end, end_ore) = (format!("_{s}"), format!("_{s}ore"));
    harvestables
        .iter()
        .filter(|(name, _)| name.ends_with(&end) || name.ends_with(&end_ore))
        .min_by_key(|(name, _)| name.chars().count())
        .map(|(_, guid)| *guid)
}

/// The goods with a quality of the DataForge `bytes`, named from the English `global.ini` `ini`.
///
/// # Errors
///
/// [`Error::DataForge`] when the bytes are no readable DataForge.
pub fn goods_from_dataforge(bytes: &[u8], ini: &[u8]) -> Result<Vec<Good>, Error> {
    let forge = DataForge::open(bytes)
        .map_err(|source| Error::DataForge { entry: "Data\\Game2.dcb".to_string(), source: Box::new(source) })?;
    Ok(extract(&forge, ini))
}

/// Size and modification time of the archive as `<bytes>:<nanoseconds since 1970>`; `None` without the file.
pub fn archive_stamp(archive: &Path) -> Option<String> {
    let meta = std::fs::metadata(archive).ok()?;
    let modified = meta.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?;
    Some(format!("{}:{}", meta.len(), modified.as_nanos()))
}

/// The text of [`FILE`] for the goods read from an archive with `stamp`.
pub fn file_text(stamp: &str, goods: &[Good]) -> String {
    let entries = goods
        .iter()
        .map(|good| {
            Json::Array(vec![
                Json::str(&good.name),
                Json::str(good.unit.key()),
                good.piece_scu.map_or(Json::Null, Json::Float),
            ])
        })
        .collect();
    let data = Json::Object(vec![
        ("format".into(), Json::int(FORMAT)),
        ("stand".into(), Json::str(stamp)),
        ("waren".into(), Json::Array(entries)),
    ]);
    jsonio::dumps(&data, Some(1), false)
}

/// What keeping the list current did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Keep {
    /// The file belongs to the archive as it is: nothing was read.
    Current,
    /// The archive was read and the file written anew.
    Written,
    /// There is no archive to read.
    NoArchive,
    /// So few goods were found that the database was not understood; the file stays as it was.
    TooFew(usize),
    /// The write lock of a copy of the data refused the file.
    Refused,
    /// Reading or writing failed; the file stays as it was.
    Failed(String),
}

/// Whether `file` was read from an archive with `stamp`, in the current layout.
pub fn is_current(file: &str, stamp: &str) -> bool {
    let Ok(read) = jsonio::read_json(file) else { return false };
    read.get("stand").and_then(Json::as_str) == Some(stamp) && read.get("format") == Some(&Json::int(FORMAT))
}

/// Reads the goods with `read` and writes `file` when it does not belong to the archive at `archive` as it is now.
pub fn keep_current_with(archive: &Path, file: &str, read: impl FnOnce() -> Result<Vec<Good>, Error>) -> Keep {
    let Some(stamp) = archive_stamp(archive) else { return Keep::NoArchive };
    if is_current(file, &stamp) {
        return Keep::Current;
    }
    let goods = match read() {
        Ok(goods) => goods,
        Err(error) => return Keep::Failed(error.to_string()),
    };
    if goods.len() < MINIMUM {
        return Keep::TooFew(goods.len());
    }
    match jsonio::write_text_atomic(file, &file_text(&stamp, &goods), Newline::Keep) {
        Ok(()) => Keep::Written,
        Err(error) if versekit_core::writeguard::refused(&error) => Keep::Refused,
        Err(error) => Keep::Failed(error.to_string()),
    }
}

/// Keeps `file` current for the archive at `archive`: reads the game database (seconds) only after a patch.
pub fn keep_current(archive: &Path, file: &str) -> Keep {
    keep_current_with(archive, file, || {
        let game = crate::GameData::open_file(archive)?;
        goods_from_dataforge(&game.dataforge_bytes()?, &game.english_ini_bytes()?)
    })
}

/// Keeps the list of the data folder of `paths` current for its archive and notes what went wrong: a failure in
/// the error log, too few goods in the start trail. Without a game nothing happens.
pub fn keep(paths: &versekit_core::paths::Paths) -> Option<Keep> {
    let archive = paths.archive_file()?;
    let outcome = keep_current(Path::new(&archive), &paths.app_file(FILE));
    match &outcome {
        Keep::Failed(reason) => versekit_core::errors::record("quality_goods.refresh", reason),
        Keep::TooFew(count) => {
            versekit_core::trail::step(&format!("Waren mit Güte: nur {count} erkannt, nicht abgelegt"));
        }
        _ => {}
    }
    Some(outcome)
}

#[cfg(test)]
mod tests;
