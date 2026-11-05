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

//! The game's text file per channel: which translation, fetching it,
//! keeping it current. Port of `translation.py`.
//!
//! Star Citizen reads `data/Localization/<language>/global.ini` before the
//! texts in its `Data.p4k`; the community translations rest on that.
//! Nothing of them is shipped: each is fetched at run time from its own
//! address, only when the player picks it. The own address of the player
//! works the same way.
//!
//! Every channel (LIVE, HOTFIX, PTU, EPTU, TECH-PREVIEW) can have its own source. The main
//! installation keeps its notes under the bare source name, a side channel
//! under `source@CHANNEL`.
//!
//! Before a file is replaced, the old one is copied beside it
//! (`global.ini.versekit-<time>`), the newest [`BACKUPS_KEPT`] stay; before the
//! language line goes into a `user.cfg` for the first time, the old file is
//! kept as `user.cfg.versekit-vorher`.

use std::sync::LazyLock;

use crate::errors;
use crate::game_ini::{self, GAME_LANGUAGES};
use crate::injection;
use crate::jsonio::{self, Map, Value, map_set};
use crate::language::{self, Arg, Lang};
use crate::net::{FetchError, Net};
use crate::paths::Paths;
use crate::text::{decode_ignore, py_splitlines, py_strip};
use crate::zip;

pub const NOTE_FILE: &str = game_ini::NOTE_FILE;
pub const USER_AGENT: &str = "SC-BP-Watcher (+https://github.com/Xharig/VerseKit)";
/// Seconds one request may take.
pub const TIMEOUT_SECONDS: u64 = 60;
/// The own address and the source of a side channel.
pub const CUSTOM: &str = game_ini::CUSTOM;
pub const ORIGINAL: &str = game_ini::ORIGINAL;
pub const SETTING_SOURCE: &str = game_ini::SETTING_SOURCE;
pub const SETTING_CUSTOM: &str = game_ini::SETTING_CUSTOM;
pub const SETTING_CHANNELS: &str = "kanal_quellen";
/// Channels a translation can be picked for.
pub const CHANNELS: [&str; 5] = ["LIVE", "HOTFIX", "PTU", "EPTU", "TECH-PREVIEW"];
/// Channels that get the test version of a source.
pub const TEST_CHANNELS: [&str; 3] = ["PTU", "EPTU", "TECH-PREVIEW"];
/// The field of a note with the version box of the file as it was placed ([`injection::menu_text_of`]).
const HEAD: &str = "kopf";
/// More than this nobody loads as a text file.
pub const CUSTOM_MAX_BYTES: usize = 80 * 1_048_576;
/// Copies of a replaced `global.ini` kept beside it.
pub const BACKUPS_KEPT: usize = 3;
/// The one-time copy of a `user.cfg` before the first change.
pub const USER_CFG_BACKUP: &str = "user.cfg.versekit-vorher";
const LUFTWERFT: &str = "https://www.luftwerft.com/download/resources/localizations";

/// The test version of a source: what differs from the live one.
#[derive(Clone, Debug, Default)]
pub struct Test {
    pub tag: Option<&'static str>,
    pub asset: Option<&'static str>,
    pub part: Option<&'static str>,
    pub url: Option<String>,
}

/// A source of translations (`translation.SOURCES`).
#[derive(Clone, Debug, Default)]
pub struct Source {
    pub id: &'static str,
    /// GitHub repository, or none for a file at a direct address.
    pub repo: Option<&'static str>,
    /// The release asset.
    pub asset: Option<&'static str>,
    pub tag: Option<&'static str>,
    /// The folder inside the archive (`LIVE/`).
    pub part: Option<&'static str>,
    pub url: Option<String>,
    /// The language folder of the game.
    pub language: &'static str,
    /// The voice language (`g_languageAudio`), or none to leave it.
    pub audio: Option<&'static str>,
    pub license: &'static str,
    pub page: &'static str,
    /// Text key of its name.
    pub label: &'static str,
    pub flag: &'static str,
    /// Text key of a note of the source, shown before switching.
    pub hint: Option<&'static str>,
    pub test: Option<Test>,
}

fn dymerz(
    id: &'static str,
    asset: &'static str,
    language: &'static str,
    label: &'static str,
    flag: &'static str,
) -> Source {
    Source {
        id,
        repo: Some("Dymerz/StarCitizen-Localization"),
        asset: Some(asset),
        language,
        audio: Some("english"),
        license: "siehe Projektseite",
        page: "https://github.com/Dymerz/StarCitizen-Localization",
        label,
        flag,
        ..Source::default()
    }
}

fn luftwerft(id: &'static str, language: &'static str, label: &'static str, flag: &'static str) -> Source {
    Source {
        id,
        url: Some(format!("{LUFTWERFT}/LIVE/{language}/global.ini")),
        language,
        audio: if language == "english" { None } else { Some("english") },
        license: "siehe Projektseite",
        page: "https://www.luftwerft.com/",
        label,
        flag,
        hint: Some("s_sp_hinweis_luftwerft"),
        test: Some(Test { url: Some(format!("{LUFTWERFT}/PTU/{language}/global.ini")), ..Test::default() }),
        ..Source::default()
    }
}

/// The sources, in the order of `translation.SOURCES`.
pub static SOURCES: LazyLock<Vec<Source>> = LazyLock::new(|| {
    vec![
        Source {
            id: "starstrings",
            repo: Some("MrKraken/StarStrings"),
            asset: Some("StarStrings-LIVE.zip"),
            language: "english",
            license: "siehe Projektseite",
            page: "https://github.com/MrKraken/StarStrings",
            label: "s_sp_q_ss",
            flag: "gb",
            test: Some(Test { tag: Some("latest-ptu"), asset: Some("StarStrings-PTU.zip"), ..Test::default() }),
            ..Source::default()
        },
        dymerz("dymerz_de", "german_.germany.zip", "german_(germany)", "s_sp_q_dymerz_de", "de"),
        dymerz("dymerz_fr", "french_.france.zip", "french_(france)", "s_sp_q_fr", "fr"),
        dymerz("dymerz_it", "italian_.italy.zip", "italian_(italy)", "s_sp_q_it", "it"),
        dymerz("dymerz_es", "spanish_.spain.zip", "spanish_(spain)", "s_sp_q_es", "es"),
        dymerz("dymerz_pt", "portuguese_.brazil.zip", "portuguese_(brazil)", "s_sp_q_pt", "br"),
        dymerz("dymerz_tr", "turkish_.turkey.zip", "turkish_(turkey)", "s_sp_q_tr", "tr"),
        luftwerft("luftwerft_de", "german_(germany)", "s_sp_q_lw_de", "de"),
        luftwerft("luftwerft_en", "english", "s_sp_q_lw_en", "gb"),
        luftwerft("luftwerft_fr", "french_(france)", "s_sp_q_lw_fr", "fr"),
        luftwerft("luftwerft_it", "italian_(italy)", "s_sp_q_lw_it", "it"),
        luftwerft("luftwerft_es", "spanish_(spain)", "s_sp_q_lw_es", "es"),
        luftwerft("luftwerft_bar", "bavarian_(germany)", "s_sp_q_lw_bar", "de"),
        luftwerft("luftwerft_nl", "dutch_(netherlands)", "s_sp_q_lw_nl", "nl"),
        luftwerft("luftwerft_pt", "portuguese_(portugal)", "s_sp_q_lw_pt", "pt"),
    ]
});

/// A source by its id.
pub fn source(id: &str) -> Option<&'static Source> {
    SOURCES.iter().find(|s| s.id == id)
}

/// The key of a note: the bare source for the main installation,
/// `source@CHANNEL` for a side channel.
pub fn note_key(source: &str, channel: Option<&str>) -> String {
    match channel {
        Some(channel) if !channel.is_empty() => format!("{source}@{channel}"),
        _ => source.to_string(),
    }
}

// ----------------------------------------------------- own address

/// `translation._custom_settings`: address and folder for a channel.
pub fn custom_settings(paths: &Paths, channel: Option<&str>) -> Map {
    let settings = paths.settings();
    let Some(Value::Object(all)) = settings.iter().find(|(k, _)| k == SETTING_CUSTOM).map(|(_, v)| v) else {
        return Map::new();
    };
    let slot = channel.filter(|c| !c.is_empty()).unwrap_or("MAIN");
    match all.iter().find(|(k, _)| k == slot).map(|(_, v)| v) {
        Some(Value::Object(entry)) => entry.clone(),
        _ => Map::new(),
    }
}

/// `translation.normalize_url`: only `https://`; a GitHub file view becomes
/// the raw file.
pub fn normalize_url(url: &str) -> Option<String> {
    let url = py_strip(url);
    if !url.to_lowercase().starts_with("https://") {
        return None;
    }
    let marker = "https://github.com/";
    if let Some(rest) = url.strip_prefix(marker)
        && let Some((owner_repo, file)) = rest.split_once("/blob/")
    {
        return Some(format!("https://raw.githubusercontent.com/{owner_repo}/{file}"));
    }
    Some(url.to_string())
}

/// `translation.set_custom`: remember the own address of a channel.
/// Returns the cleaned address, or `None` when it is unusable.
pub fn set_custom(paths: &Paths, url: &str, language: &str, channel: Option<&str>) -> Option<String> {
    let url = normalize_url(url)?;
    let settings = paths.settings();
    let mut all = match settings.iter().find(|(k, _)| k == SETTING_CUSTOM).map(|(_, v)| v) {
        Some(Value::Object(map)) => map.clone(),
        _ => Map::new(),
    };
    let language = if GAME_LANGUAGES.contains(&language) { language } else { "english" };
    let entry = vec![("url".to_string(), Value::str(&url)), ("sprache".to_string(), Value::str(language))];
    map_set(&mut all, channel.filter(|c| !c.is_empty()).unwrap_or("MAIN"), Value::Object(entry));
    paths.set_setting(SETTING_CUSTOM, Value::Object(all));
    Some(url)
}

// ----------------------------------------------- source per channel

/// What is fetched for a source in a channel.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Spec {
    pub source: String,
    pub repo: Option<String>,
    pub asset: Option<String>,
    pub tag: Option<String>,
    pub part: Option<String>,
    pub url: Option<String>,
    pub language: String,
    pub audio: Option<String>,
    pub label: String,
    pub hint: Option<String>,
}

fn owned(value: Option<&str>) -> Option<String> {
    value.map(str::to_string)
}

/// `translation._spec`: `None` when the source has no version for the
/// channel.
pub fn spec(paths: &Paths, source: &str, channel: Option<&str>) -> Option<Spec> {
    if source == CUSTOM {
        let custom = custom_settings(paths, channel);
        let url = custom.iter().find(|(k, _)| k == "url").and_then(|(_, v)| v.as_str()).filter(|u| !u.is_empty())?;
        let language = custom
            .iter()
            .find(|(k, _)| k == "sprache")
            .and_then(|(_, v)| v.as_str())
            .filter(|l| !l.is_empty())
            .unwrap_or("english");
        return Some(Spec {
            source: CUSTOM.into(),
            repo: None,
            asset: None,
            tag: None,
            part: None,
            url: Some(url.to_string()),
            language: language.to_string(),
            audio: None,
            label: "s_sp_q_eigen".into(),
            hint: None,
        });
    }
    let q = self::source(source)?;
    let mut spec = Spec {
        source: q.id.into(),
        repo: owned(q.repo),
        asset: owned(q.asset),
        tag: owned(q.tag),
        part: owned(q.part),
        url: q.url.clone(),
        language: q.language.into(),
        audio: owned(q.audio),
        label: q.label.into(),
        hint: owned(q.hint),
    };
    if channel.is_some_and(|c| TEST_CHANNELS.contains(&c)) {
        let test = q.test.as_ref()?;
        if let Some(tag) = test.tag {
            spec.tag = Some(tag.into());
        }
        if let Some(asset) = test.asset {
            spec.asset = Some(asset.into());
        }
        if let Some(part) = test.part {
            spec.part = Some(part.into());
        }
        if let Some(url) = &test.url {
            spec.url = Some(url.clone());
        }
    }
    Some(spec)
}

/// `translation.available`.
pub fn available(paths: &Paths, source: &str, channel: Option<&str>) -> bool {
    source == ORIGINAL || source == CUSTOM || spec(paths, source, channel).is_some()
}

/// `translation.language_folder`: where the source writes.
pub fn language_folder(paths: &Paths, source: &str, channel: Option<&str>) -> String {
    if source == ORIGINAL {
        return "english".into();
    }
    let noted = info(paths, source, channel)
        .iter()
        .find(|(k, _)| k == "sprache")
        .and_then(|(_, v)| v.as_str())
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    if source == CUSTOM
        && let Some(language) = &noted
    {
        return language.clone();
    }
    match spec(paths, source, channel) {
        Some(spec) if !spec.language.is_empty() => spec.language,
        Some(_) => "english".into(),
        None => noted.unwrap_or_else(|| "english".into()),
    }
}

/// The selectable sources in groups of one language each.
pub fn grouped_sources(paths: &Paths, channel: Option<&str>, with_original: bool) -> Vec<Vec<String>> {
    let mut by_language: Vec<(String, Vec<String>)> = Vec::new();
    let mut push = |language: &str, id: &str| match by_language.iter_mut().find(|(l, _)| l == language) {
        Some((_, list)) => list.push(id.to_string()),
        None => by_language.push((language.to_string(), vec![id.to_string()])),
    };
    for q in SOURCES.iter() {
        if available(paths, q.id, channel) {
            // Bavarian is a German language: it stands in the row of German.
            let group = if q.language.starts_with("bavarian") { "german_(germany)" } else { q.language };
            push(group, q.id);
        }
    }
    if with_original {
        push("english", ORIGINAL);
    }
    // German, then English, then the rest by name.
    let rank = |language: &str| {
        if language.starts_with("german") {
            0
        } else if language.starts_with("english") {
            1
        } else {
            2
        }
    };
    // In a row: Luftwerft, StarStrings, the others, and Bavarian last behind the German ones.
    let source_rank = |id: &str| {
        if id == "luftwerft_bar" {
            3
        } else if id.starts_with("luftwerft_") {
            0
        } else if id == "starstrings" {
            1
        } else {
            2
        }
    };
    by_language.sort_by(|a, b| (rank(&a.0), &a.0).cmp(&(rank(&b.0), &b.0)));
    by_language
        .into_iter()
        .map(|(_, mut list)| {
            list.sort_by_key(|id| source_rank(id));
            list
        })
        .collect()
}

/// The text key of a source's name (`translation.display_name`); an
/// unknown source is its own id.
pub fn label_key(source: &str) -> String {
    match source {
        CUSTOM => "s_sp_q_eigen".into(),
        ORIGINAL => "s_sp_q_or".into(),
        id => self::source(id).map_or_else(|| id.to_string(), |q| q.label.to_string()),
    }
}

/// The country whose flag stands before a source: its language, the
/// original texts English; none for an own address (4.12 `flagge`).
pub fn flag_of(source: &str) -> Option<&'static str> {
    match source {
        ORIGINAL => Some("gb"),
        id => self::source(id).map(|q| q.flag).filter(|flag| !flag.is_empty()),
    }
}

/// The name of a source as the player reads it.
pub fn display_name(source: &str) -> String {
    let key = label_key(source);
    if source == CUSTOM || source == ORIGINAL || self::source(source).is_some() { language::t(&key) } else { key }
}

/// `translation.channel_sources`: {channel: source} of the side channels.
pub fn channel_sources(paths: &Paths) -> Map {
    let settings = paths.settings();
    match settings.iter().find(|(k, _)| k == SETTING_CHANNELS).map(|(_, v)| v) {
        Some(Value::Object(map)) => map.clone(),
        _ => Map::new(),
    }
}

/// `translation.set_channel_source`; an empty source removes the entry.
pub fn set_channel_source(paths: &Paths, channel: &str, source: &str) -> bool {
    let mut map = channel_sources(paths);
    if source.is_empty() {
        map.retain(|(k, _)| k != channel);
    } else {
        map_set(&mut map, channel, Value::str(source));
    }
    paths.set_setting(SETTING_CHANNELS, Value::Object(map))
}

/// Source ids that are no longer offered, each with the source that takes its place (same language folder).
pub const RETIRED_SOURCES: [(&str, &str); 2] = [("deutsch", "luftwerft_de"), ("thord82_es", "luftwerft_es")];

/// The source that takes the place of a retired one, if `id` is retired.
fn successor(id: &str) -> Option<&'static str> {
    RETIRED_SOURCES.iter().find(|(old, _)| *old == id).map(|(_, new)| *new)
}

/// Moves the settings and notes of retired sources ([`RETIRED_SOURCES`]) to the source that takes their place: the
/// chosen source of the main installation and of the side channels, the notes of placed files and the list of
/// confirmed sources. A moved note keeps the identifier of the old file, so the next look finds a newer version and
/// fetches the file of the new source. When the new source has a note of its own, the old note replaces it only
/// where the old source was the chosen one (its file is the one in the game folder); otherwise the old note goes.
/// Returns whether anything changed.
pub fn retire_sources(paths: &Paths) -> bool {
    let mut changed = false;
    let main_was_old = paths.setting(SETTING_SOURCE).is_some_and(|s| successor(&s).is_some());
    if let Some(new) = paths.setting(SETTING_SOURCE).as_deref().and_then(successor) {
        paths.set_setting(SETTING_SOURCE, Value::str(new));
        changed = true;
    }
    let mut channels = channel_sources(paths);
    let mut old_channels: Vec<String> = Vec::new();
    for (channel, value) in &mut channels {
        if let Some(new) = value.as_str().and_then(successor) {
            *value = Value::str(new);
            old_channels.push(channel.clone());
        }
    }
    if !old_channels.is_empty() {
        paths.set_setting(SETTING_CHANNELS, Value::Object(channels));
        changed = true;
    }
    let mut data = note(paths);
    let old_keys: Vec<(String, String, bool)> = data
        .iter()
        .filter_map(|(key, _)| {
            let (source, channel) = match key.split_once('@') {
                Some((source, channel)) => (source, Some(channel)),
                None => (key.as_str(), None),
            };
            let new = successor(source)?;
            let chosen = match channel {
                Some(channel) => old_channels.iter().any(|c| c == channel),
                None => main_was_old,
            };
            Some((key.clone(), note_key(new, channel), chosen))
        })
        .collect();
    for (old_key, new_key, chosen) in &old_keys {
        let Some(at) = data.iter().position(|(k, _)| k == old_key) else { continue };
        let (_, entry) = data.remove(at);
        let taken = data.iter().any(|(k, _)| k == new_key);
        if !taken || *chosen {
            let kept: Map = match entry {
                Value::Object(fields) => {
                    fields.into_iter().filter(|(k, _)| ["kennung", "stand", "sprache"].contains(&k.as_str())).collect()
                }
                _ => Map::new(),
            };
            map_set(&mut data, new_key, Value::Object(kept));
        }
    }
    if !old_keys.is_empty() {
        note_write(paths, data);
        changed = true;
    }
    let confirmed = paths.setting(crate::game_texts::SETTING_CONFIRMED).unwrap_or_default();
    let list: Vec<&str> = confirmed.split(',').filter(|s| !s.is_empty()).collect();
    if list.iter().any(|s| successor(s).is_some()) {
        let kept: Vec<&str> = list.into_iter().filter(|s| successor(s).is_none()).collect();
        paths.set_setting(crate::game_texts::SETTING_CONFIRMED, Value::str(kept.join(",")));
        changed = true;
    }
    changed
}

// ------------------------------------------------------- the notes

/// `translation._note`.
pub fn note(paths: &Paths) -> Map {
    match jsonio::read_json(&paths.app_file(NOTE_FILE)) {
        Ok(Value::Object(map)) => map,
        _ => Map::new(),
    }
}

fn note_write(paths: &Paths, data: Map) {
    let text = jsonio::dumps(&Value::Object(data), Some(1), false);
    if let Err(error) = jsonio::write_text_atomic(&paths.app_file(NOTE_FILE), &text, jsonio::Newline::Native) {
        errors::record("translation._note_write", &error);
    }
}

/// `time.strftime('%Y-%m-%d %H:%M')`.
fn minute_stamp() -> String {
    injection::now_stamp()[..16].to_string()
}

/// `translation._note_set`: the entry is replaced whole.
pub fn note_set(paths: &Paths, key: &str, ident: Option<&str>, language: Option<&str>, date: Option<&str>) {
    let mut data = note(paths);
    let stand = minute_stamp();
    let mut entry =
        vec![("kennung".to_string(), ident.map_or(Value::Null, Value::str)), ("stand".to_string(), Value::str(&stand))];
    if let Some(language) = language.filter(|l| !l.is_empty()) {
        entry.push(("sprache".into(), Value::str(language)));
    }
    if let Some(date) = date.filter(|d| !d.is_empty()) {
        entry.push(("datum".into(), Value::str(date)));
        entry.push(("geprueft".into(), Value::str(&stand)));
        entry.push(("pruefung".into(), Value::str("aktuell")));
    }
    map_set(&mut data, key, Value::Object(entry));
    note_write(paths, data);
}

/// What a look for a newer version found, and how fetching it ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Check {
    Current,
    Newer,
    Failed,
    /// The fetch was refused by the lock of a run on a copy of the data.
    Refused,
    /// The fetch failed; the reason stands beside it (`grund`).
    FetchFailed,
    /// On a copy of the data a newer version was found and, on purpose, not placed; its day stands beside it
    /// (`neu_datum`).
    NotPlaced,
}

impl Check {
    fn word(self) -> &'static str {
        match self {
            Check::Current => "aktuell",
            Check::Newer => "neu",
            Check::Failed => "fehler",
            Check::Refused => "gesperrt",
            Check::FetchFailed => "holen_fehler",
            Check::NotPlaced => "testbetrieb",
        }
    }
}

/// The field of a note with the day of a newer version found on a copy of the data.
const NEW_DATE: &str = "neu_datum";

/// What a look on a copy of the data found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Looked {
    Current,
    /// A newer version, with its day (`YYYY-MM-DD`) where the source names one; not placed.
    Newer(Option<String>),
}

impl Looked {
    /// What the page says about it in `lang`: calm, a lock that is wanted is no failure.
    pub fn text_in(&self, lang: Lang) -> String {
        match self {
            Looked::Current => language::t_in(lang, "inj_aktuell"),
            Looked::Newer(Some(date)) => {
                language::tf_in(lang, "s_sp_test_neu_kurz", &[("0", Arg::from(day(date, lang)))])
            }
            Looked::Newer(None) => language::t_in(lang, "s_sp_test_neu_kurz_ohne"),
        }
    }
}

/// Whether a translation for the game folder `root` is only looked at, never fetched: on a copy of the data, or
/// wherever the write lock of such a run would refuse the game folder (a thread that took the lock over builds
/// its own paths and does not know the copy).
pub fn only_looks(paths: &Paths, root: &str) -> bool {
    crate::testmode::on_copy(paths) || crate::writeguard::would_refuse(&paths.host.flavor.join(root, "data"))
}

/// On a copy of the data only looks for the newest version of a source: the header of a direct address or the
/// release on GitHub, never the file itself. Nothing is downloaded and nothing goes into the game folder, the
/// write lock of the copy would refuse it anyway. The note keeps what was found: current (with the day of the
/// version), or a newer version not placed (with its day).
///
/// # Errors
///
/// The problem of the look (no version for the channel, the net).
pub fn look_on_copy(net: &dyn Net, paths: &Paths, source: &str, channel: Option<&str>) -> Result<Looked, Problem> {
    let key = note_key(source, channel);
    let found = spec(paths, source, channel)
        .ok_or_else(|| Problem::NoChannelVersion(channel.unwrap_or("LIVE").to_string()))
        .and_then(|spec| latest(net, &spec));
    let found = match found {
        Ok(found) => found,
        Err(problem) => {
            record_check(paths, &key, Check::Failed, None);
            return Err(problem);
        }
    };
    let placed = installed(paths, source, channel);
    if found.ident.is_some() && placed.as_deref() == found.ident.as_deref() {
        record_check(paths, &key, Check::Current, found.date.as_deref());
        return Ok(Looked::Current);
    }
    record_check(paths, &key, Check::NotPlaced, None);
    let mut data = note(paths);
    if let Some((_, Value::Object(entry))) = data.iter_mut().find(|(k, _)| *k == key) {
        entry.retain(|(k, _)| k != NEW_DATE);
        if let Some(date) = found.date.as_deref().filter(|d| !d.is_empty()) {
            map_set(entry, NEW_DATE, Value::str(date));
        }
        note_write(paths, data);
    }
    Ok(Looked::Newer(found.date))
}

/// `translation._record_check`: only an entry that is there.
fn record_check(paths: &Paths, key: &str, result: Check, date: Option<&str>) {
    record_check_with(paths, key, result, date, None);
}

/// [`record_check`] with the reason of a failed fetch; any reason noted before goes.
fn record_check_with(paths: &Paths, key: &str, result: Check, date: Option<&str>, reason: Option<&str>) {
    let mut data = note(paths);
    let Some((_, Value::Object(entry))) = data.iter_mut().find(|(k, _)| k == key) else { return };
    map_set(entry, "geprueft", Value::str(minute_stamp()));
    map_set(entry, "pruefung", Value::str(result.word()));
    entry.retain(|(k, _)| k != REASON);
    if let Some(reason) = reason {
        map_set(entry, REASON, Value::str(reason));
    }
    if result == Check::Current
        && let Some(date) = date.filter(|d| !d.is_empty())
    {
        map_set(entry, "datum", Value::str(date));
    }
    note_write(paths, data);
}

/// The field of a note with the reason a fetch failed.
const REASON: &str = "grund";

/// Note how a fetch ended that did not place a file: refused by the lock, or failed for a reason.
fn record_fetch_problem(paths: &Paths, key: &str, problem: &Problem) {
    match problem {
        Problem::TestRun => record_check(paths, key, Check::Refused, None),
        other => record_check_with(paths, key, Check::FetchFailed, None, Some(other.word())),
    }
}

/// The fetches running in this process: (note file, note key).
static FETCHING: std::sync::Mutex<Vec<(String, String)>> = std::sync::Mutex::new(Vec::new());

/// A fetch of one note key that runs as long as this value lives.
struct Fetching(String, String);

impl Fetching {
    fn start(paths: &Paths, key: &str) -> Fetching {
        let entry = (paths.app_file(NOTE_FILE), key.to_string());
        if let Ok(mut list) = FETCHING.lock() {
            list.push(entry.clone());
        }
        Fetching(entry.0, entry.1)
    }
}

impl Drop for Fetching {
    fn drop(&mut self) {
        if let Ok(mut list) = FETCHING.lock()
            && let Some(at) = list.iter().position(|(file, key)| *file == self.0 && *key == self.1)
        {
            list.remove(at);
        }
    }
}

/// Whether a fetch of this note key runs right now in this process.
pub fn fetching(paths: &Paths, key: &str) -> bool {
    let file = paths.app_file(NOTE_FILE);
    FETCHING.lock().is_ok_and(|list| list.iter().any(|(f, k)| *f == file && k == key))
}

/// `translation.info`: the note of a source, empty without one.
pub fn info(paths: &Paths, source: &str, channel: Option<&str>) -> Map {
    let key = note_key(source, channel);
    match note(paths).into_iter().find(|(k, _)| *k == key) {
        Some((_, Value::Object(entry))) => entry,
        _ => Map::new(),
    }
}

/// `translation.installed`: the identifier of the version placed, if any.
pub fn installed(paths: &Paths, source: &str, channel: Option<&str>) -> Option<String> {
    match info(paths, source, channel).into_iter().find(|(k, _)| k == "kennung").map(|(_, v)| v) {
        Some(Value::Str(s)) if !s.is_empty() => Some(s),
        Some(Value::Int(n)) if n != "0" => Some(n),
        _ => None,
    }
}

/// `translation.note`: a source counts as set up (also the original texts).
pub fn note_source(paths: &Paths, key: &str, ident: &str, language: Option<&str>) {
    note_set(paths, key, Some(ident), language, None);
}

/// `translation.forget_note`.
pub fn forget_note(paths: &Paths, source: &str) {
    let mut data = note(paths);
    let before = data.len();
    data.retain(|(k, _)| k != source);
    if data.len() != before {
        note_write(paths, data);
    }
}

/// Notes the version box of the file just placed for the note `key` ([`injection::menu_text_of`]): by it a file
/// another program puts there later is told from the one VerseKit placed ([`replaced_by_other`]). Only an entry that
/// is there; a file without a version box notes `null`.
/// Where the version box names the stand of the translation's data (`Data Version: DD.MM.YYYY HH:MM`, as
/// Luftwerft writes it) and the build of the game it is made for (`4.10.2-live.12913470`), those go into the note
/// too (`datum` as `YYYY-MM-DD HH:MM`, `build`): the page says the stand of the file, not only when it was fetched.
pub fn note_head(paths: &Paths, key: &str, bytes: &[u8]) {
    let head = injection::menu_text_of(bytes);
    let mut data = note(paths);
    let Some((_, Value::Object(entry))) = data.iter_mut().find(|(k, _)| k == key) else { return };
    if let Some(stand) = head.as_deref().and_then(data_stand) {
        map_set(entry, "datum", Value::str(stand));
    }
    if let Some(build) = head.as_deref().and_then(build_of) {
        map_set(entry, BUILD, Value::str(build));
    }
    map_set(entry, HEAD, head.map_or(Value::Null, Value::str));
    note_write(paths, data);
}

/// The field of a note with the build of the game the placed file names.
const BUILD: &str = "build";

/// The stand of the data a version box names (`Data Version: 04.11.2026 13:09`), as `2026-11-04 13:09`.
pub fn data_stand(head: &str) -> Option<String> {
    static STAND: LazyLock<regex::Regex> = LazyLock::new(|| {
        regex::Regex::new(r"(?i)data\s*version:\s*(\d{1,2})\.(\d{1,2})\.(\d{4})(?:\s+(\d{1,2}):(\d{2}))?")
            .expect("pattern")
    });
    let found = STAND.captures(head)?;
    let number = |i: usize| found.get(i).and_then(|m| m.as_str().parse::<u32>().ok());
    let (day, month, year) = (number(1)?, number(2)?, number(3)?);
    if !(1..=31).contains(&day) || !(1..=12).contains(&month) {
        return None;
    }
    Some(match (number(4), number(5)) {
        (Some(hour), Some(minute)) => format!("{year:04}-{month:02}-{day:02} {hour:02}:{minute:02}"),
        _ => format!("{year:04}-{month:02}-{day:02}"),
    })
}

/// The build of the game a version box names (`4.10.2-live.12913470`, `4.10.3-ptu.13093654`).
pub fn build_of(head: &str) -> Option<String> {
    static BUILD_NAME: LazyLock<regex::Regex> = LazyLock::new(|| {
        regex::Regex::new(r"(?i)\b\d+\.\d+(?:\.\d+)*-(?:live|ptu|eptu|hotfix|tech-preview|evocati)\.\d+\b")
            .expect("pattern")
    });
    BUILD_NAME.find(head).map(|m| m.as_str().to_string())
}

/// [`note_head`] with the file at `target` as it is now.
pub fn note_head_of_file(paths: &Paths, key: &str, target: &str) {
    match std::fs::read(target) {
        Ok(bytes) => note_head(paths, key, &bytes),
        Err(error) => errors::record("translation.note_head", &error),
    }
}

/// Whether the note of `source` in `channel` holds the version box of its file.
pub fn head_noted(paths: &Paths, source: &str, channel: Option<&str>) -> bool {
    info(paths, source, channel).iter().any(|(k, _)| k == HEAD)
}

/// Whether the text file `target` of `source` in `channel` is no longer the one VerseKit placed, because another
/// program wrote its own file there: its version box (without our line) differs from the one noted at placing, or,
/// where none was noted (a file placed by an older version), it holds words only another program writes there
/// ([`injection::FOREIGN_MENU_WORDS`]). A file that cannot be read tells nothing.
pub fn replaced_by_other(paths: &Paths, source: &str, channel: Option<&str>, target: &str) -> bool {
    let Ok(bytes) = std::fs::read(target) else { return false };
    let head = injection::menu_text_of(&bytes);
    let entry = info(paths, source, channel);
    match entry.iter().find(|(k, _)| k == HEAD).map(|(_, v)| v) {
        Some(noted) => noted.as_str() != head.as_deref(),
        None => head.is_some_and(|head| injection::FOREIGN_MENU_WORDS.iter().any(|word| head.contains(word))),
    }
}

/// `translation._day`: `YYYY-MM-DD` as `DD.MM.YYYY` in German.
fn day(stamp: &str, lang: Lang) -> String {
    let parts: Vec<&str> = stamp.get(..10).unwrap_or(stamp).split('-').collect();
    let [y, m, d] = parts[..] else { return String::new() };
    if lang == Lang::De { format!("{d}.{m}.{y}") } else { format!("{y}-{m}-{d}") }
}

/// A stamp `YYYY-MM-DD HH:MM` as the status line writes it: `today HH:MM` for today, else the day and the time.
fn when_text(stamp: &str, lang: Lang, today: &str) -> String {
    let clock = stamp.get(11..16).unwrap_or("");
    if stamp.get(..10) == Some(today) {
        language::tf_in(lang, "s_sp_heute", &[("0", Arg::from(clock))])
    } else {
        format!("{} {clock}", day(stamp, lang))
    }
}

/// `translation.status_text`: date of the version, when it was fetched and whether it is
/// current, never the identifier. `today` is `YYYY-MM-DD`.
pub fn status_text(paths: &Paths, source: &str, channel: Option<&str>, lang: Lang, today: &str) -> String {
    let entry = info(paths, source, channel);
    let text = |key: &str| entry.iter().find(|(k, _)| k == key).and_then(|(_, v)| v.as_str()).unwrap_or("").to_string();
    let mut parts = Vec::new();
    // First the stand of the file (its day, with the time where the source names one, and the build it is for),
    // then when it was fetched and looked at.
    let date = text("datum");
    let build = text(BUILD);
    if !date.is_empty() {
        let mut stand = day(&date, lang);
        if let Some(clock) = date.get(11..16) {
            stand = format!("{stand} {clock}");
        }
        parts.push(if build.is_empty() {
            language::tf_in(lang, "s_sp_stand_vom", &[("0", Arg::from(stand))])
        } else {
            language::tf_in(lang, "s_sp_stand_build", &[("0", Arg::from(stand)), ("1", Arg::from(build.as_str()))])
        });
    } else if !build.is_empty() {
        parts.push(language::tf_in(lang, "s_sp_fuer_build", &[("0", Arg::from(build.as_str()))]));
    }
    let fetched = text("stand");
    if !fetched.is_empty() {
        parts.push(language::tf_in(lang, "s_sp_geholt", &[("0", Arg::from(when_text(&fetched, lang, today)))]));
    }
    let checked = text("geprueft");
    if !checked.is_empty() {
        let when = when_text(&checked, lang, today);
        // "Being fetched" only while a fetch of this source really runs; a newer version that was found but is
        // not being fetched says so.
        let note = note_key(source, channel);
        let key = match text("pruefung").as_str() {
            "aktuell" => Some("s_sp_ist_aktuell"),
            "neu" if fetching(paths, &note) => Some("s_sp_neuere_da"),
            "neu" => Some("s_sp_neuere_offen"),
            "fehler" => Some("s_sp_nicht_geprueft"),
            "gesperrt" => Some("s_sp_holen_gesperrt"),
            "testbetrieb" => {
                let new_date = text(NEW_DATE);
                parts.push(if new_date.is_empty() {
                    language::tf_in(lang, "s_sp_test_neu_ohne", &[("0", Arg::from(when.as_str()))])
                } else {
                    language::tf_in(
                        lang,
                        "s_sp_test_neu",
                        &[("0", Arg::from(when.as_str())), ("1", Arg::from(day(&new_date, lang)))],
                    )
                });
                None
            }
            "holen_fehler" => {
                let reason = Problem::from_word(&text(REASON), channel).unwrap_or(Problem::Net(String::new()));
                parts.push(language::tf_in(
                    lang,
                    "s_sp_holen_fehl",
                    &[("0", Arg::from(when.as_str())), ("1", Arg::from(reason.short_text_in(lang)))],
                ));
                None
            }
            _ => None,
        };
        if let Some(key) = key {
            parts.push(language::tf_in(lang, key, &[("0", Arg::from(when))]));
        }
    }
    parts.join(" · ")
}

/// Today as `YYYY-MM-DD`.
pub fn today() -> String {
    injection::now_stamp()[..10].to_string()
}

// ------------------------------------------------- looking for new

/// Why nothing could be fetched or looked up.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Problem {
    /// The source has no version for this channel (`m_keine_kanalfassung`).
    NoChannelVersion(String),
    /// No version to download found (`m_keine_fassung`).
    NoVersion,
    /// The system's certificates are missing (`m_kein_zertifikat`).
    Certificate,
    /// The site refused (403, `m_abgewiesen`).
    Refused,
    /// Fetching is switched off.
    Offline,
    /// Another network error, as text.
    Net(String),
    /// Under the own address there is no text file of the game (`m_keine_spieldatei`).
    NotGameFile,
    /// The archive holds no `global.ini` (`m_keine_ini_archiv`).
    NoIniInArchive,
    /// No game folder (`s_tq_kein_spiel`).
    NoGameFolder,
    /// A run on a copy of the data writes nothing into the game folder (`schutz_spielordner`).
    TestRun,
    /// Writing into the game folder failed; `denied` when the system refused the permission.
    WriteFailed { denied: bool },
}

impl Problem {
    /// The text for the player.
    pub fn text(&self) -> String {
        self.text_in(language::current())
    }

    /// The text for the player in `lang`; a network error with the words it came with.
    pub fn text_in(&self, lang: Lang) -> String {
        match self {
            Problem::Net(text) if !text.is_empty() => text.clone(),
            other => other.short_text_in(lang),
        }
    }

    /// The text for the player in `lang`, a network error without its technical words.
    pub fn short_text_in(&self, lang: Lang) -> String {
        match self {
            Problem::NoChannelVersion(channel) => {
                language::tf_in(lang, "m_keine_kanalfassung", &[("0", Arg::from(channel.as_str()))])
            }
            Problem::NoVersion => language::t_in(lang, "m_keine_fassung"),
            Problem::Certificate => language::t_in(lang, "m_kein_zertifikat"),
            Problem::Refused => language::t_in(lang, "m_abgewiesen"),
            Problem::Offline => language::t_in(lang, "m_h_kein_netz"),
            Problem::Net(_) => language::t_in(lang, "m_netz_fehl"),
            Problem::NotGameFile => language::t_in(lang, "m_keine_spieldatei"),
            Problem::NoIniInArchive => language::t_in(lang, "m_keine_ini_archiv"),
            Problem::NoGameFolder => language::t_in(lang, "s_tq_kein_spiel"),
            Problem::TestRun => language::t_in(lang, "schutz_spielordner"),
            Problem::WriteFailed { denied: true } => language::t_in(lang, "s_tq_keine_rechte"),
            Problem::WriteFailed { denied: false } => language::t_in(lang, "s_tq_schreiben_fehl"),
        }
    }

    /// The word a note keeps for the reason (`grund`).
    fn word(&self) -> &'static str {
        match self {
            Problem::NoChannelVersion(_) => "kanal",
            Problem::NoVersion => "fassung",
            Problem::Certificate => "zertifikat",
            Problem::Refused => "abgewiesen",
            Problem::Offline => "offline",
            Problem::Net(_) => "netz",
            Problem::NotGameFile => "spieldatei",
            Problem::NoIniInArchive => "archiv",
            Problem::NoGameFolder => "spielordner",
            Problem::TestRun => "testbetrieb",
            Problem::WriteFailed { denied: true } => "rechte",
            Problem::WriteFailed { denied: false } => "schreiben",
        }
    }

    /// The reason a note keeps, back as a problem; `channel` is the one of the note.
    fn from_word(word: &str, channel: Option<&str>) -> Option<Problem> {
        Some(match word {
            "kanal" => Problem::NoChannelVersion(channel.unwrap_or("LIVE").to_string()),
            "fassung" => Problem::NoVersion,
            "zertifikat" => Problem::Certificate,
            "abgewiesen" => Problem::Refused,
            "offline" => Problem::Offline,
            "netz" => Problem::Net(String::new()),
            "spieldatei" => Problem::NotGameFile,
            "archiv" => Problem::NoIniInArchive,
            "spielordner" => Problem::NoGameFolder,
            "testbetrieb" => Problem::TestRun,
            "rechte" => Problem::WriteFailed { denied: true },
            "schreiben" => Problem::WriteFailed { denied: false },
            _ => return None,
        })
    }

    /// A writing into the game folder that failed: the lock of a copy of the data, a permission the system
    /// refused, or anything else (that one goes to the error log with its words).
    fn of_write(error: &std::io::Error) -> Problem {
        if crate::writeguard::refused(error) {
            Problem::TestRun
        } else if error.kind() == std::io::ErrorKind::PermissionDenied {
            Problem::WriteFailed { denied: true }
        } else {
            errors::record("translation.fetch", error);
            Problem::WriteFailed { denied: false }
        }
    }
}

/// `translation._net_error`.
fn net_problem(error: &FetchError) -> Problem {
    match error {
        FetchError::Refused => Problem::Refused,
        FetchError::Offline => Problem::Offline,
        other => {
            let text = other.to_string();
            let upper = text.to_uppercase();
            if upper.contains("CERTIFICATE") || upper.contains("SSL") {
                Problem::Certificate
            } else if text.contains("403") {
                Problem::Refused
            } else {
                Problem::Net(text)
            }
        }
    }
}

/// The newest version of a source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Latest {
    /// Release tag, commit or header value; `None` when a direct address
    /// names nothing.
    pub ident: Option<String>,
    pub url: String,
    pub size: u64,
    /// The day of that version (`YYYY-MM-DD`), where known.
    pub date: Option<String>,
}

fn is_direct(spec: &Spec) -> bool {
    spec.source == CUSTOM || (spec.url.is_some() && spec.repo.is_none())
}

fn get_json(net: &dyn Net, url: &str) -> Result<Value, Problem> {
    let bytes = net.get(url).map_err(|e| net_problem(&e))?;
    jsonio::loads(&String::from_utf8_lossy(&bytes)).map_err(|e| Problem::Net(e.to_string()))
}

/// `translation._custom_latest`: the identifier from the header only.
fn head_ident(net: &dyn Net, url: &str) -> Result<Option<String>, Problem> {
    let headers = net.head(url).map_err(|e| net_problem(&e))?;
    for (field, prefix) in [("etag", "etag"), ("last-modified", "datum"), ("content-length", "groesse")] {
        if let Some((_, value)) = headers.iter().find(|(k, _)| k == field) {
            let value = py_strip(value).trim_matches('"');
            if !value.is_empty() {
                return Ok(Some(format!("{prefix}:{value}")));
            }
        }
    }
    Ok(None)
}

fn str_of(value: Option<&Value>) -> &str {
    value.and_then(Value::as_str).unwrap_or("")
}

/// `translation.latest`.
pub fn latest(net: &dyn Net, spec: &Spec) -> Result<Latest, Problem> {
    if is_direct(spec) {
        let url = spec.url.clone().unwrap_or_default();
        return Ok(Latest { ident: head_ident(net, &url)?, url, size: 0, date: None });
    }
    let repo = spec.repo.as_deref().unwrap_or("");
    let release = match &spec.tag {
        Some(tag) => get_json(net, &format!("https://api.github.com/repos/{repo}/releases/tags/{tag}"))?,
        None => get_json(net, &format!("https://api.github.com/repos/{repo}/releases/latest"))?,
    };
    let mut ident = str_of(release.get("tag_name")).to_string();
    let published = str_of(release.get("published_at"));
    let lower = ident.to_lowercase();
    if lower.is_empty() || lower == "latest" || lower.starts_with("latest-") {
        ident = published.chars().take(19).collect();
    }
    let date = (!published.is_empty()).then(|| published.chars().take(10).collect());
    if let Some(Value::Array(assets)) = release.get("assets") {
        for asset in assets {
            if Some(str_of(asset.get("name"))) == spec.asset.as_deref() {
                let size = match asset.get("size") {
                    Some(Value::Int(n)) => n.parse().unwrap_or(0),
                    _ => 0,
                };
                return Ok(Latest {
                    ident: Some(ident),
                    url: str_of(asset.get("browser_download_url")).to_string(),
                    size,
                    date,
                });
            }
        }
    }
    Err(Problem::NoVersion)
}

/// `translation.update_available`: (newer, identifier). Never fails; the
/// result of the look is noted.
pub fn update_available(net: &dyn Net, paths: &Paths, source: &str, channel: Option<&str>) -> (bool, Option<String>) {
    let key = note_key(source, channel);
    let found = spec(paths, source, channel)
        .ok_or_else(|| Problem::NoChannelVersion(channel.unwrap_or("LIVE").to_string()))
        .and_then(|spec| latest(net, &spec));
    let Ok(Latest { ident: Some(ident), date, .. }) = found else {
        record_check(paths, &key, Check::Failed, None);
        return (false, None);
    };
    let newer = installed(paths, source, channel).as_deref() != Some(ident.as_str());
    record_check(paths, &key, if newer { Check::Newer } else { Check::Current }, date.as_deref());
    (newer, Some(ident))
}

// ---------------------------------------------------------- placing

/// `translation._ini_from_zip`: the `global.ini` of an archive and the
/// language folder it names (`None` for a file without folder).
pub fn ini_from_zip(
    content: &[u8],
    language: &str,
    part: Option<&str>,
) -> Result<(Option<Vec<u8>>, Option<String>), zip::BadZip> {
    let entries = zip::entries(content)?;
    let normal = |name: &str| name.replace('\\', "/");
    let mut names: Vec<&zip::Entry> =
        entries.iter().filter(|e| normal(&e.name).to_lowercase().rsplit('/').next() == Some("global.ini")).collect();
    if let Some(part) = part {
        names.retain(|e| normal(&e.name).to_lowercase().starts_with(&part.to_lowercase()));
    }
    for entry in &names {
        let lowered = normal(&entry.name).to_lowercase();
        if !language.is_empty() && lowered.split('/').any(|p| p == language.to_lowercase()) {
            return Ok((Some(zip::read(content, entry)?), Some(language.to_string())));
        }
    }
    if names.len() == 1 {
        let path = normal(&names[0].name);
        let parts: Vec<&str> = path.split('/').collect();
        let lowered: Vec<String> = parts.iter().map(|p| p.to_lowercase()).collect();
        let found = lowered
            .iter()
            .position(|p| p == "localization")
            .filter(|&i| i + 1 < parts.len() - 1)
            .map(|i| parts[i + 1].to_lowercase());
        return Ok((Some(zip::read(content, names[0])?), found));
    }
    Ok((None, None))
}

fn find(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}

/// `translation.looks_like_global_ini`: many `key=text` lines and the
/// vehicle names, no web page, not too large.
pub fn looks_like_global_ini(content: &[u8]) -> bool {
    if content.is_empty() || content.len() > CUSTOM_MAX_BYTES {
        return false;
    }
    let head = &content[..content.len().min(20_000_000)];
    let start = head[..head.len().min(2000)].to_ascii_lowercase();
    if find(&start, b"<html") {
        return false;
    }
    find(head, b"vehicle_") && head.iter().filter(|&&b| b == b'=').count() > 5000
}

/// The mark of a copy beside a game file: `<file>.versekit-<tail>`.
const BACKUP_MARK: &str = ".versekit-";
/// The mark of copies an older version left; they are found and can be put
/// back, but none is made any more.
const OLD_BACKUP_MARK: &str = ".scbpw-";

/// The tail of a copy's name after its mark: `YYYYMMDD-HHMMSS`, within the
/// same second with a counter `-NN`.
fn is_backup_tail(tail: &str) -> bool {
    let bytes = tail.as_bytes();
    let stamp_ok = |s: &[u8]| s[8] == b'-' && s.iter().enumerate().all(|(i, b)| i == 8 || b.is_ascii_digit());
    match bytes.len() {
        15 => stamp_ok(bytes),
        18 => stamp_ok(&bytes[..15]) && bytes[15] == b'-' && bytes[16..].iter().all(u8::is_ascii_digit),
        _ => false,
    }
}

/// Beside a copy: the note of original texts that belonged to the copied
/// file, so that putting the copy back can reset its notes again.
fn companion(backup: &str) -> String {
    format!("{backup}{BACKUP_NOTE}")
}

const BACKUP_NOTE: &str = ".urtext.json";

/// `paths.backup_game_file` for the text file: a copy beside it as
/// `<file>.versekit-<YYYYMMDD-HHMMSS>`; [`replace_game_file`] cuts the old ones
/// after a writing that worked. When the note of original texts belongs to
/// this file, it is kept beside the copy. Returns the copy, or `None` when
/// there was nothing to keep.
///
/// # Errors
///
/// When the copy fails; then nothing may be written.
pub fn backup_game_file(paths: &Paths, target: &str) -> std::io::Result<Option<String>> {
    if !game_ini::is_file(target) {
        return Ok(None);
    }
    let mut stamp = jsonio::time_stamp();
    // A clock that went back (the end of summer time, a changed zone) must not put the new copy before the
    // older ones: it takes the time of the newest one and counts on from there.
    if let Some(latest) = backups_of(target).last().map(|b| file_name(b))
        && let Some(from) =
            [BACKUP_MARK, OLD_BACKUP_MARK].iter().find_map(|mark| latest.rfind(mark).map(|at| at + mark.len()))
        && let Some(latest_stamp) = latest.get(from..from + 15)
        && latest_stamp > stamp.as_str()
    {
        stamp = latest_stamp.to_string();
    }
    let base = format!("{target}{BACKUP_MARK}{stamp}");
    let base_name = file_name(&base);
    // Within the same second a counter behind the newest copy, so that the
    // names keep sorting by age. Compared by name: the folder may be spelt
    // with other separators.
    let newest = backups_of(target)
        .iter()
        .filter_map(|b| file_name(b).strip_prefix(base_name.as_str()).map(str::to_string))
        .map(|rest| rest.strip_prefix('-').and_then(|n| n.parse::<u32>().ok()).unwrap_or(1))
        .max();
    let backup = match newest {
        None => base,
        Some(n) => format!("{base}-{:02}", n + 1),
    };
    jsonio::copy2(target, &backup)?;
    if let Some(note) = injection::origtext_note_of(paths, target) {
        let text = jsonio::dumps(&Value::Object(note), None, false);
        if let Err(error) = jsonio::write_text_atomic(&companion(&backup), &text, jsonio::Newline::Keep) {
            let _ = jsonio::remove_file(&backup);
            return Err(error);
        }
    }
    Ok(Some(backup))
}

fn file_name(path: &str) -> String {
    std::path::Path::new(path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

/// Replace a game file after a copy of it; only when the writing worked are
/// the oldest copies cut to `keep`. A writing that fails takes its copy back,
/// so that failed tries (a write-protected file, again every five minutes) do not
/// push the real way back out.
///
/// # Errors
///
/// When the copy or the writing fails; then the file is as it was.
pub fn replace_game_file(paths: &Paths, target: &str, bytes: &[u8], keep: usize) -> std::io::Result<Option<String>> {
    let backup = backup_game_file(paths, target)?;
    if let Err(error) = injection::write_game_file(target, bytes) {
        if let Some(copy) = &backup
            && let Err(cleanup) = remove_backup(copy)
        {
            errors::record("translation.replace_game_file", &cleanup);
        }
        return Err(error);
    }
    prune_backups(target, keep, backup.as_deref());
    Ok(backup)
}

/// The copies of a game file, oldest first by the time in their names; also
/// the copies of an older version, which count like the own ones (so the
/// cutting never removes the only copy there is).
pub fn backups_of(target: &str) -> Vec<String> {
    let path = std::path::Path::new(target);
    let (Some(folder), Some(name)) = (path.parent(), path.file_name().and_then(|n| n.to_str())) else {
        return Vec::new();
    };
    let marks = [(0, format!("{name}{OLD_BACKUP_MARK}")), (1, format!("{name}{BACKUP_MARK}"))];
    let mut found: Vec<(String, u8, String)> = std::fs::read_dir(folder)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| e.file_name().to_str().map(str::to_string))
        .filter_map(|n| {
            marks
                .iter()
                .find_map(|(order, prefix)| {
                    n.strip_prefix(prefix.as_str())
                        .filter(|tail| is_backup_tail(tail))
                        .map(|tail| (tail.to_string(), *order))
                })
                .map(|(tail, order)| (tail, order, n))
        })
        .collect();
    found.sort();
    found.into_iter().map(|(_, _, n)| folder.join(n).to_string_lossy().into_owned()).collect()
}

/// The copy first, then its note: a copy left over must never lose its note.
fn remove_backup(backup: &str) -> std::io::Result<()> {
    jsonio::remove_file(backup)?;
    let note = companion(backup);
    if std::path::Path::new(&note).exists()
        && let Err(error) = jsonio::remove_file(&note)
    {
        errors::record("translation.remove_backup", &error);
    }
    Ok(())
}

/// Cuts the copies to `keep`, the oldest first; the one just made (`fresh`) is never cut.
fn prune_backups(target: &str, keep: usize, fresh: Option<&str>) {
    // Compared by name: the folder may be spelt with other separators.
    let fresh_name = fresh.map(file_name);
    let all: Vec<String> = backups_of(target).into_iter().filter(|b| Some(file_name(b)) != fresh_name).collect();
    let room = keep.saturating_sub(usize::from(fresh.is_some()));
    for old in &all[..all.len().saturating_sub(room)] {
        if let Err(error) = remove_backup(old) {
            errors::record("translation.prune_backups", &error);
        }
    }
}

/// Put the newest copy of a game file back. Returns the copy used.
///
/// For a text file (`main`, in any channel) its note of original texts follows: the one
/// kept beside the copy, so its notes can be reset again. A copy without one
/// held no notes the note knew of; then the note of this file is emptied but
/// not marked fresh, and the next writing finds leftovers by their form
/// (only blocks with our boxes, never a bare title mark of another tool). A
/// note of another file that is still there stays: that file needs it.
///
/// Once the file is back, the step counts as done: a copy that cannot be
/// removed is recorded, and it keeps its note for a later try.
///
/// # Errors
///
/// When there is no copy or it cannot be written back.
pub fn restore_backup(paths: &Paths, target: &str, main: bool) -> std::io::Result<String> {
    let _one_writer = injection::game_file_lock();
    let newest = backups_of(target).pop().ok_or_else(|| std::io::Error::from(std::io::ErrorKind::NotFound))?;
    let bytes = std::fs::read(&newest)?;
    let kept = match jsonio::read_json(&companion(&newest)) {
        Ok(Value::Object(note)) => Some(note),
        _ => None,
    };
    injection::write_game_file(target, &bytes)?;
    let mut noted = true;
    if main && !injection::origtext_held_for_other(paths, target) {
        noted = match kept {
            Some(note) => injection::put_origtext_note(paths, target, note),
            None => injection::save_origtext(paths, Map::new(), target),
        };
    }
    // Without its note written, the copy and its note stay for a later try.
    if noted && let Err(error) = remove_backup(&newest) {
        errors::record("translation.restore_backup", &error);
    }
    Ok(newest)
}

/// `translation.set_user_cfg`: set `g_language` (and `g_languageAudio`)
/// in the `user.cfg` of a channel folder, line by line; everything else
/// stays. Before the first change the old file is kept once beside it.
pub fn set_user_cfg(paths: &Paths, root: &str, language: &str, audio: Option<&str>) -> bool {
    let known = |l: &str| GAME_LANGUAGES.contains(&l.to_lowercase().as_str());
    if !game_ini::is_language_folder(language) || audio.is_some_and(|a| !known(a)) || root.is_empty() {
        return false;
    }
    let flavor = paths.host.flavor;
    let path = flavor.join(root, "user.cfg");
    let mut lines: Vec<String> = Vec::new();
    if game_ini::is_file(&path) {
        match std::fs::read(&path) {
            Ok(bytes) => lines = py_splitlines(&decode_ignore(&bytes)).into_iter().map(str::to_string).collect(),
            Err(_) => return false,
        }
    }
    let mut set_language = false;
    let mut set_audio = audio.is_none();
    let mut fresh = Vec::with_capacity(lines.len() + 2);
    for line in &lines {
        let key = py_strip(line.split('=').next().unwrap_or(""));
        if key == "g_language" {
            fresh.push(format!("g_language = {language}"));
            set_language = true;
        } else if let Some(audio) = audio.filter(|_| key == "g_languageAudio") {
            fresh.push(format!("g_languageAudio = {audio}"));
            set_audio = true;
        } else {
            fresh.push(line.clone());
        }
    }
    if !set_language {
        fresh.push(format!("g_language = {language}"));
    }
    if let Some(audio) = audio.filter(|_| !set_audio) {
        fresh.push(format!("g_languageAudio = {audio}"));
    }
    if fresh == lines {
        return true;
    }
    let backup = flavor.join(root, USER_CFG_BACKUP);
    if game_ini::is_file(&path)
        && !game_ini::is_file(&backup)
        && std::fs::metadata(&path).is_ok_and(|m| m.len() > 0)
        && let Err(error) = jsonio::copy2(&path, &backup)
    {
        errors::record("translation.set_user_cfg", &error);
        return false;
    }
    let text = jsonio::translate(&(fresh.join("\n") + "\n"), jsonio::Newline::Native);
    // A file of the game: a write protection stays and the writing fails.
    match injection::write_game_file(&path, text.as_bytes()) {
        Ok(()) => true,
        Err(error) => {
            errors::record("translation.set_user_cfg", &error);
            false
        }
    }
}

/// What a fetch placed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fetched {
    pub source: String,
    pub ident: String,
    pub bytes: usize,
    pub target: String,
    pub language: String,
    /// The copy of the file that was there before, if any.
    pub backup: Option<String>,
}

/// A step the fetch reports while it runs.
#[derive(Clone, Debug, PartialEq)]
pub enum Progress {
    /// Loading: name and megabytes (`z_laedt`).
    Loading(String, f64),
    /// Placing the file (`z_einsetzen`).
    Placing,
}

/// `translation.fetch`: fetch a source and place it. `root` is the folder of
/// the channel; `channel` is `None` for the main installation. The stored
/// original texts of the channel's note are discarded (a fresh base file), and
/// the version box of the file is noted ([`note_head`]).
///
/// While it runs, [`fetching`] answers yes for its note; a fetch that places
/// nothing notes why (refused by the lock of a copy of the data, or failed).
pub fn fetch(
    net: &dyn Net,
    paths: &Paths,
    source: &str,
    root: &str,
    channel: Option<&str>,
    progress: &dyn Fn(Progress),
) -> Result<Fetched, Problem> {
    let key = note_key(source, channel);
    let running = Fetching::start(paths, &key);
    let result = fetch_and_place(net, paths, source, root, channel, progress);
    drop(running);
    if let Err(problem) = &result {
        record_fetch_problem(paths, &key, problem);
    }
    result
}

fn fetch_and_place(
    net: &dyn Net,
    paths: &Paths,
    source: &str,
    root: &str,
    channel: Option<&str>,
    progress: &dyn Fn(Progress),
) -> Result<Fetched, Problem> {
    let key = note_key(source, channel);
    let spec =
        spec(paths, source, channel).ok_or_else(|| Problem::NoChannelVersion(channel.unwrap_or("LIVE").to_string()))?;
    let found = latest(net, &spec)?;
    progress(Progress::Loading(language::t(&spec.label), found.size as f64 / 1_048_576.0));
    let content = net.get(&found.url).map_err(|e| net_problem(&e))?;
    let mut language = spec.language.clone();
    let ini = if content.starts_with(b"PK") {
        match ini_from_zip(&content, &language, spec.part.as_deref()) {
            Ok((ini, from_archive)) => {
                if source == CUSTOM
                    && let Some(folder) = from_archive
                {
                    language = folder;
                }
                ini
            }
            Err(_) => None,
        }
    } else {
        let head = &content[..content.len().min(20_000_000)];
        find(head, b"\nvehicle_").then_some(content.clone())
    };
    let direct = is_direct(&spec);
    if direct && !ini.as_deref().is_some_and(looks_like_global_ini) {
        return Err(Problem::NotGameFile);
    }
    let ini = ini.ok_or(Problem::NoIniInArchive)?;
    let ident = match found.ident {
        Some(ident) => ident,
        None => {
            let digest = ring::digest::digest(&ring::digest::SHA256, &ini);
            let hex: String = digest.as_ref().iter().map(|b| format!("{b:02x}")).collect();
            format!("inhalt:{}", &hex[..16])
        }
    };
    let target = game_ini::target_ini_in(paths, root, &language).ok_or(Problem::NoGameFolder)?;
    progress(Progress::Placing);
    let _one_writer = injection::game_file_lock();
    let flavor = paths.host.flavor;
    jsonio::make_dirs(&flavor.dirname(&target)).map_err(|e| Problem::of_write(&e))?;
    let backup = replace_game_file(paths, &target, &ini, BACKUPS_KEPT).map_err(|e| Problem::of_write(&e))?;
    set_user_cfg(paths, root, &language, spec.audio.as_deref());
    note_set(paths, &key, Some(&ident), Some(&language), found.date.as_deref());
    note_head(paths, &key, &ini);
    // A fresh base file, in the note of original texts of its own channel.
    injection::discard_origtext_for(paths, &target);
    Ok(Fetched { source: source.to_string(), ident, bytes: ini.len(), target, language, backup })
}

#[cfg(test)]
mod tests;
