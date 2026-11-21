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

//! The list of translations Luftwerft offers: `{"LIVE": {"<folder>": {"url": …}}, "PTU": {…}}`.
//!
//! The fixed sources of [`crate::game_ini::SOURCES`] stand without it; the list adds the languages that are not
//! among them, so a new language appears without a new version of the program. The last good answer is kept in
//! the data folder (with its tag, so the next look is a free `304`); a broken or missing list changes nothing.
//! Only addresses under [`BASE`] are taken.

use std::collections::BTreeMap;
use std::sync::{LazyLock, Mutex};
use std::time::SystemTime;

use serde_json::{Value, json};

use crate::errors;
use crate::game_ini;
use crate::jsonio;
use crate::net::{Fresh, Net};
use crate::paths::Paths;

/// The kept answer in the data folder.
pub const FILE: &str = "luftwerft_sprachen.json";
/// The list.
pub const LIST_URL: &str =
    "https://www.luftwerft.com/download/resources/localizations/_protocol/protocol_file_translations.json";
/// Every address of a file starts with this.
pub const BASE: &str = "https://www.luftwerft.com/download/resources/localizations/";
/// The prefix of the id of every source from Luftwerft.
pub const ID_PREFIX: &str = "luftwerft_";

/// A language folder of the list with its addresses for the main installation (`LIVE`) and the test channels
/// (`PTU`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub folder: String,
    pub live: Option<String>,
    pub ptu: Option<String>,
}

/// The last good list.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Listing {
    /// The tag the server gave the list.
    pub etag: Option<String>,
    /// What the list holds now.
    pub listed: Vec<Entry>,
    /// Folders that were listed once, are not among the fixed sources and are gone from the list now.
    pub gone: Vec<Entry>,
}

/// How a look at the list ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refresh {
    /// A new list was taken.
    Changed,
    /// The server still has the list that is kept.
    Unchanged,
    /// Nothing could be fetched, or the answer was no usable list; what was kept stands.
    Failed,
}

/// Is `folder` one the fixed sources already serve.
pub fn is_fixed(folder: &str) -> bool {
    game_ini::SOURCES.iter().any(|(id, fixed)| id.starts_with(ID_PREFIX) && *fixed == folder)
}

fn valid_folder(folder: &str) -> bool {
    !folder.is_empty()
        && folder.len() <= 64
        && folder.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '_' | '(' | ')' | '-'))
}

fn valid_url(url: &str) -> bool {
    url.starts_with(BASE)
        && url.ends_with("/global.ini")
        && !url.contains("..")
        && url.chars().all(|c| c.is_ascii_graphic() && !matches!(c, '?' | '#' | '\\' | '"' | '\''))
}

/// The folders of an answer of the server, each with its addresses; `None` when the answer is no list or holds
/// no usable entry. Anything but an `https://` address under [`BASE`] is dropped.
pub fn parse(bytes: &[u8]) -> Option<Vec<Entry>> {
    let value: Value = serde_json::from_slice(bytes).ok()?;
    let object = value.as_object()?;
    let mut entries: Vec<Entry> = Vec::new();
    for channel in ["LIVE", "PTU"] {
        let Some(languages) = object.get(channel).and_then(Value::as_object) else { continue };
        for (folder, entry) in languages {
            let url = entry.get("url").and_then(Value::as_str).unwrap_or("");
            if !valid_folder(folder) || !valid_url(url) {
                continue;
            }
            let at = match entries.iter().position(|e| e.folder == *folder) {
                Some(at) => at,
                None => {
                    entries.push(Entry { folder: folder.clone(), live: None, ptu: None });
                    entries.len() - 1
                }
            };
            let slot = if channel == "LIVE" { &mut entries[at].live } else { &mut entries[at].ptu };
            *slot = Some(url.to_string());
        }
    }
    (!entries.is_empty()).then_some(entries)
}

fn entry_to_value(entry: &Entry) -> Value {
    json!({"ordner": entry.folder, "live": entry.live, "ptu": entry.ptu})
}

fn entry_from_value(value: &Value) -> Option<Entry> {
    let folder = value.get("ordner").and_then(Value::as_str)?;
    let url = |key: &str| value.get(key).and_then(Value::as_str).filter(|u| valid_url(u)).map(str::to_string);
    valid_folder(folder).then(|| Entry { folder: folder.to_string(), live: url("live"), ptu: url("ptu") })
}

fn from_value(value: &Value) -> Option<Listing> {
    let list = |key: &str| -> Vec<Entry> {
        value
            .get(key)
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(entry_from_value).collect())
            .unwrap_or_default()
    };
    let listing = Listing {
        etag: value.get("etag").and_then(Value::as_str).filter(|t| !t.is_empty()).map(str::to_string),
        listed: list("liste"),
        gone: list("frueher"),
    };
    (!listing.listed.is_empty()).then_some(listing)
}

fn to_value(listing: &Listing) -> Value {
    json!({
        "etag": listing.etag,
        "liste": listing.listed.iter().map(entry_to_value).collect::<Vec<_>>(),
        "frueher": listing.gone.iter().map(entry_to_value).collect::<Vec<_>>(),
    })
}

type Memo = BTreeMap<String, (Option<(SystemTime, u64)>, Option<Listing>)>;

/// The kept lists by file, with the stamp of the file they were read at, so a look does not read it again.
static MEMO: LazyLock<Mutex<Memo>> = LazyLock::new(|| Mutex::new(Memo::new()));

fn stamp(path: &str) -> Option<(SystemTime, u64)> {
    let meta = std::fs::metadata(path).ok()?;
    Some((meta.modified().ok()?, meta.len()))
}

/// The kept list; `None` while there is none (the fixed sources stand alone).
pub fn load(paths: &Paths) -> Option<Listing> {
    let path = paths.app_file(FILE);
    let now = stamp(&path);
    let mut memo = MEMO.lock().ok()?;
    if let Some((was, listing)) = memo.get(&path)
        && *was == now
    {
        return listing.clone();
    }
    let listing = std::fs::read(&path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        .and_then(|v| from_value(&v));
    memo.insert(path, (now, listing.clone()));
    listing
}

fn store(paths: &Paths, listing: &Listing) {
    let path = paths.app_file(FILE);
    let text = to_value(listing).to_string();
    if let Err(error) = jsonio::write_text_atomic(&path, &text, jsonio::Newline::Keep) {
        errors::record("luftwerft_list.store", &error);
        return;
    }
    if let Ok(mut memo) = MEMO.lock() {
        memo.insert(path.clone(), (stamp(&path), Some(listing.clone())));
    }
}

/// The new list on top of the old one: what disappeared (and is no fixed source) is remembered as gone.
fn merge(before: Option<Listing>, entries: Vec<Entry>, etag: Option<String>) -> Listing {
    let mut gone: Vec<Entry> = Vec::new();
    if let Some(before) = before {
        for old in before.listed.into_iter().chain(before.gone) {
            let back = entries.iter().any(|e| e.folder == old.folder);
            if !back && !is_fixed(&old.folder) && !gone.iter().any(|g| g.folder == old.folder) {
                gone.push(old);
            }
        }
    }
    Listing { etag, listed: entries, gone }
}

/// Looks at the list with the tag of the last answer and keeps a new one. A list that cannot be fetched or read
/// leaves what was kept as it is.
pub fn refresh(net: &dyn Net, paths: &Paths) -> Refresh {
    let before = load(paths);
    let etag = before.as_ref().and_then(|l| l.etag.clone());
    match net.get_if_changed(LIST_URL, etag.as_deref()) {
        Ok(Fresh::Unchanged) if before.is_some() => Refresh::Unchanged,
        Ok(Fresh::Unchanged) => Refresh::Failed,
        Ok(Fresh::Body(bytes, tag)) => {
            let Some(entries) = parse(&bytes) else { return Refresh::Failed };
            let listing = merge(before.clone(), entries, tag);
            if before.as_ref() == Some(&listing) {
                return Refresh::Unchanged;
            }
            store(paths, &listing);
            Refresh::Changed
        }
        Err(_) => Refresh::Failed,
    }
}

/// The folders that are not among the fixed sources: the listed ones, then those gone from the list, each part
/// by folder.
pub fn extras(paths: &Paths) -> Vec<Entry> {
    let Some(listing) = load(paths) else { return Vec::new() };
    let mut out: Vec<Entry> = Vec::new();
    for part in [&listing.listed, &listing.gone] {
        let mut part: Vec<&Entry> = part.iter().filter(|e| !is_fixed(&e.folder)).collect();
        part.sort_by(|a, b| a.folder.cmp(&b.folder));
        for entry in part {
            if !out.iter().any(|e| e.folder == entry.folder) {
                out.push(entry.clone());
            }
        }
    }
    out
}

/// Does the list name `folder` for the channel kind (`test`: the PTU address, else the LIVE one)? `None` while no
/// list is kept, since then nothing is known.
pub fn lists(paths: &Paths, folder: &str, test: bool) -> Option<bool> {
    let listing = load(paths)?;
    Some(listing.listed.iter().any(|e| e.folder == folder && if test { e.ptu.is_some() } else { e.live.is_some() }))
}

/// The id of the source of a folder: `luftwerft_` and the folder with everything but letters and digits
/// squeezed to single underscores (`swedish_(sweden)` → `luftwerft_swedish_sweden`).
pub fn id_of(folder: &str) -> String {
    let mut slug = String::new();
    for c in folder.chars() {
        if c.is_ascii_alphanumeric() {
            slug.push(c.to_ascii_lowercase());
        } else if !slug.ends_with('_') {
            slug.push('_');
        }
    }
    format!("{ID_PREFIX}{}", slug.trim_matches('_'))
}

fn capitalised(text: &str) -> String {
    text.split(['_', ' '])
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut chars = w.chars();
            chars.next().map_or_else(String::new, |first| first.to_uppercase().chain(chars).collect())
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// A name read from the folder, for a language the program has no name for: `swedish_(sweden)` →
/// `Swedish, Sweden (Luftwerft)`; the folder `english` → `English (Luftwerft)`.
pub fn name_of(folder: &str) -> String {
    let (language, region) = match folder.split_once("_(") {
        Some((language, rest)) => (language, rest.trim_end_matches(')')),
        None => (folder, ""),
    };
    let language = capitalised(language);
    let region = capitalised(region);
    if region.is_empty() { format!("{language} (Luftwerft)") } else { format!("{language}, {region} (Luftwerft)") }
}

#[cfg(test)]
pub(crate) mod tests;
