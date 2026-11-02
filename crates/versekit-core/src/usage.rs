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

//! Once a day "I am running", and which pages of the main window were opened
//! how (`usage_ping.py`, `page_usage.py`): the same fields in the same form
//! as 4.12, to the same receiver (`tools/nutzung-worker/`). No identifier, no
//! name, no handle, no paths; the counters are ids of pages, routes and
//! actions only.
//!
//! In v5 it is always on: there is no switch (`nutzung_melden` is not read).
//! `SC_BP_NO_NET` switches it off, and with it the counting; a target that is
//! not `https://` (as `aus`) as well. Unlike 4.12, `http://` to the own
//! computer counts as a target, for the made-up receiver of the checks. A
//! version the receiver would not take (`5.0.0-dev`) sends nothing and
//! counts nothing, as the source start of 4.12.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::basetool::http::{Request, Transport};
use crate::jsonio::{self, Map, Newline, Value};
use crate::paths::{Host, Paths};
use crate::text::py_strip;

/// Where the report goes (`usage_ping.TARGET`).
pub const TARGET: &str = "https://nutzung-versekit.xharig.com/ping";
/// Another target for checks; anything without `https://` switches off.
pub const ENV_TARGET: &str = "SC_BP_NUTZUNG_ZIEL";
/// The UTC day of the last report that arrived (`usage_ping.LAST`).
pub const LAST_SETTING: &str = "nutzung_zuletzt";
/// The counters until the next report (`page_usage.FILE`).
pub const FILE: &str = "seitennutzung.json";
/// How often the background looks whether a report is due (`CHECK_EVERY`).
pub const CHECK_EVERY: Duration = Duration::from_secs(3600);
/// The next try after a failed one (`RETRY_AFTER`).
pub const RETRY_AFTER: Duration = Duration::from_secs(600);

/// The routes a page is opened by (`page_usage.ROUTES`). The v5 window has no tray menu and no path
/// line, so `tray` and `pfad` do not come from it.
pub const ROUTES: [&str; 7] = ["seitenleiste", "sprung", "overlay", "tray", "start", "karte", "pfad"];
/// The actions counted, only how often (`page_usage.ACTIONS`). The v5 window counts `lager_hand`,
/// `lager_raffinerie`, `lager_scan`, `galerie_offen` and `galerie_sprung`; the others belong to parts v5
/// does not have yet (card view, sync button of the stock page) and stay in the list the receiver knows.
pub const ACTIONS: [&str; 12] = [
    "lager_hand",
    "lager_raffinerie",
    "lager_scan",
    "lager_sync",
    "abgleich_karte",
    "karte_raffinerie",
    "karte_scanner",
    "karte_tr_pruefen",
    "karte_tr_neu",
    "galerie_offen",
    "galerie_sprung",
    "galerie_karten",
];
/// The page ids of the main window that are counted (`pages.page_ids()`);
/// the receiver knows these and three more of older versions. The one page "Dismantle & Salvage" is counted
/// by the view it shows: `bergung` for a wreck, `zerlegen` for the calculation of a part.
/// `statistik_auswertung` stays in the list: the receiver knows it, and the counts under it are added to
/// `statistik` by [`ALIASES`].
pub const PAGES: [&str; 61] = [
    "uebersicht",
    "liste",
    "fortschritt",
    "auftragslog",
    "hangar",
    "wunschliste",
    "asop",
    "meinlager",
    "staffellager",
    "herstellung",
    "farmliste",
    "bergbau",
    "raffinerien",
    "bergung",
    "zerlegen",
    "laeden",
    "verkauf",
    "routen",
    "bedarf",
    "einkaufsliste",
    "notizen",
    "contested",
    "statistik_auswertung",
    "statistik",
    "statistik_schiffe",
    "statistik_auftraege",
    "statistik_quantum",
    "statistik_stabil",
    "allgemein",
    "anzeige",
    "darstellung",
    "ordner",
    "module",
    "basetool",
    "bestand",
    "spiel",
    "uebersetzung",
    "grafik",
    "blickwinkel",
    "joysticks",
    "achsen",
    "shader",
    "wasistneu",
    "patchaenderungen",
    "ueber",
    "serverstatus",
    "diagnose",
    "danke",
    "erkennung",
    "startprogramme",
    "bereich_bauplaene",
    "bereich_logistik",
    "bereich_werkstatt",
    "bereich_bergung",
    "bereich_handel",
    "bereich_beschaffung",
    "bereich_statistiken",
    "bereich_einstellungen",
    "bereich_spiel_einst",
    "bereich_basetool",
    "bereich_info",
];

/// A page that is left within these seconds is a miss (`MISS_SECONDS`).
pub const MISS_SECONDS: f64 = 3.0;
/// The step for this many clicks and more (`MAX_CLICKS`).
pub const MAX_CLICKS: i64 = 10;
/// The highest value of a counter in a report (`MAX_COUNT`).
pub const MAX_COUNT: i64 = 9999;
/// So many pairs of misses go out at most (`MAX_MISS_PAIRS`).
pub const MAX_MISS_PAIRS: usize = 20;
/// So many pairs are kept at most (`MAX_STORED_PAIRS`).
pub const MAX_STORED_PAIRS: usize = 200;
/// Misses one way remembers at most (`MAX_MISSES_PER_TARGET`).
pub const MAX_MISSES_PER_TARGET: usize = 5;

const FIELDS: [&str; 5] = ["pages", "entry", "clicks", "misses", "actions"];
/// Old page names that count as their new page (`my_stock_page.ALIASES`); the evaluation page of the statistics
/// is part of the overview since 5.0.
const ALIASES: [(&str, &str); 3] =
    [("lager", "meinlager"), ("handelslager", "meinlager"), ("statistik_auswertung", "statistik")];
/// The language folders of the game and their codes (`usage_ping._GAME_LANGUAGES`).
const GAME_LANGUAGES: [(&str, &str); 11] = [
    ("english", "en"),
    ("german", "de"),
    ("french", "fr"),
    ("spanish", "es"),
    ("italian", "it"),
    ("portuguese", "pt"),
    ("polish", "pl"),
    ("chinese", "zh"),
    ("japanese", "ja"),
    ("korean", "ko"),
    ("russian", "ru"),
];
/// The names of a start with the system: the one of this program, and the earlier ones it takes over.
const AUTOSTART_NAMES: [&str; 3] =
    [crate::autostart::NAME, crate::autostart::EARLIER_NAMES[0], crate::autostart::EARLIER_NAMES[1]];

// ------------------------------------------------------------ target and switches

/// The target, if any: `SC_BP_NUTZUNG_ZIEL` (stripped) beats the built-in one.
pub fn target(host: &Host) -> Option<String> {
    let address = match host.var(ENV_TARGET) {
        Some(value) => py_strip(value).to_string(),
        None => TARGET.to_string(),
    };
    (address.starts_with("https://") || crate::report_send::loopback_http(&address)).then_some(address)
}

/// `SC_BP_NO_NET` set to anything but empty or `0`.
pub fn no_net(host: &Host) -> bool {
    host.var(crate::legacy::ENV_NO_NET).is_some_and(|value| !value.is_empty() && value != "0")
}

/// A version the receiver takes: `5.0.0`, `5.0.0-rc1` (`usage_ping.VERSION_RE`).
pub fn version_ok(version: &str) -> bool {
    let (core, rc) = match version.split_once("-rc") {
        Some((core, rc)) => (core, Some(rc)),
        None => (version, None),
    };
    let digits =
        |part: &str, most: usize| !part.is_empty() && part.len() <= most && part.bytes().all(|b| b.is_ascii_digit());
    let parts: Vec<&str> = core.split('.').collect();
    parts.len() == 3 && parts.iter().all(|p| digits(p, 3)) && rc.is_none_or(|rc| digits(rc, 4))
}

/// Would a report go out at all (`usage_ping.would_send`)? Only then pages are counted.
pub fn would_send(host: &Host, version: &str) -> bool {
    !no_net(host) && target(host).is_some() && version_ok(version)
}

/// Today in UTC, as the receiver counts (`usage_ping.today`).
pub fn today_utc(now: std::time::SystemTime) -> String {
    let days = now.duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() / 86_400) as i64;
    crate::export::utc_text(days * 86_400, false)[..10].to_string()
}

// ------------------------------------------------------------ the fields

/// The settings and states the report names besides the counters (`usage_ping.payload`).
#[derive(Clone, Debug, PartialEq)]
pub struct Facts {
    pub os: &'static str,
    pub ui: String,
    pub game: String,
    pub rc: bool,
    pub mods: Vec<String>,
    pub overlay: String,
    pub autostart: bool,
    pub update: bool,
    pub view: String,
    pub tiles_off: Vec<String>,
}

/// `windows` or `linux` (`usage_ping.system`).
pub fn system() -> &'static str {
    if cfg!(windows) { "windows" } else { "linux" }
}

/// The language the game reads, as a code: `german_(germany)` → `de`, unknown `xx`, none or
/// empty `en`.
pub fn game_code(folder: Option<&str>) -> String {
    let folder = folder.filter(|f| !f.is_empty()).unwrap_or("english").to_lowercase();
    let stem = folder.split(['_', '(']).next().unwrap_or_default();
    GAME_LANGUAGES.iter().find(|(name, _)| *name == stem).map_or("xx", |(_, code)| code).to_string()
}

/// Does the program start with the system (`autostart.is_on`)? An entry under the name of this program or
/// one of the earlier names. A made-up computer has no registry.
pub fn autostart_on(host: &Host) -> bool {
    #[cfg(windows)]
    {
        if !host.registry {
            return false;
        }
        AUTOSTART_NAMES.iter().any(|name| crate::paths::run_entry(name).is_some_and(|command| !command.is_empty()))
    }
    #[cfg(not(windows))]
    {
        let config = match host.var("XDG_CONFIG_HOME") {
            Some(folder) if !folder.is_empty() => folder.to_string(),
            _ => format!("{}/.config", host.home()),
        };
        AUTOSTART_NAMES.iter().any(|name| {
            let file = format!("{}.desktop", name.to_lowercase().replace(' ', "-"));
            std::path::Path::new(&config).join("autostart").join(file).is_file()
        })
    }
}

/// The facts as the settings and the system hold them now; `ui` is the
/// language of the window, of which the first two letters count.
pub fn facts(paths: &Paths, ui: &str) -> Facts {
    let ui: String = ui.chars().take(2).collect::<String>().to_lowercase();
    let ui = if ui.len() == 2 && ui.bytes().all(|b| b.is_ascii_lowercase()) { ui } else { "xx".into() };
    let mut mods: Vec<String> = crate::modules::SWITCHABLE
        .iter()
        .map(|(group, _)| group.to_string())
        .filter(|group| crate::modules::enabled(paths, group))
        .collect();
    mods.sort();
    let overlay = paths.setting("overlay_modus").filter(|m| m == "immer" || m == "popup").unwrap_or("immer".into());
    let mut tiles_off = crate::overview::hidden_tiles(paths);
    tiles_off.sort();
    Facts {
        os: system(),
        ui,
        game: game_code(crate::game_ini::game_language(paths).as_deref()),
        rc: paths.setting_bool("vorabversionen", false),
        mods,
        overlay,
        autostart: autostart_on(&paths.host),
        update: paths.setting_bool("update_pruefen", true) && paths.setting_bool("update_automatisch", true),
        view: if paths.setting("ansicht").as_deref() == Some("karten") { "karten" } else { "reiter" }.into(),
        tiles_off,
    }
}

/// Exactly what goes out, in the order of 4.12 (`usage_ping.payload`).
pub fn payload(version: &str, facts: &Facts, counts: &Map) -> Map {
    let strings = |items: &[String]| Value::Array(items.iter().map(Value::str).collect());
    let mut out: Map = vec![
        ("v".into(), Value::str(version)),
        ("os".into(), Value::str(facts.os)),
        ("ui".into(), Value::str(&facts.ui)),
        ("game".into(), Value::str(&facts.game)),
        ("rc".into(), Value::Bool(facts.rc)),
        ("mods".into(), strings(&facts.mods)),
        ("overlay".into(), Value::str(&facts.overlay)),
        ("autostart".into(), Value::Bool(facts.autostart)),
        ("update".into(), Value::Bool(facts.update)),
        ("view".into(), Value::str(&facts.view)),
        ("tiles_off".into(), strings(&facts.tiles_off)),
    ];
    out.extend(outgoing(&Value::Object(counts.clone())));
    out
}

/// The report as Python's `json.dumps` writes it: ASCII only, `, ` and `: `.
pub fn payload_text(payload: &Map) -> String {
    let mut out = String::new();
    encode(&mut out, &Value::Object(payload.clone()), (", ", ": "), true);
    out
}

/// The counter file as 4.12 writes it: `,` and `:`, without escaping beyond ASCII.
pub fn file_text(counts: &Map) -> String {
    let mut out = String::new();
    encode(&mut out, &Value::Object(counts.clone()), (",", ":"), false);
    out
}

/// JSON with the given separators; `ascii` escapes everything beyond ASCII as `\uXXXX`.
fn encode(out: &mut String, value: &Value, separators: (&str, &str), ascii: bool) {
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Int(n) => out.push_str(n),
        Value::Float(x) => out.push_str(&jsonio::float_text(*x)),
        Value::Str(s) => {
            if ascii {
                out.push('"');
                for c in s.chars() {
                    match c {
                        '"' => out.push_str("\\\""),
                        '\\' => out.push_str("\\\\"),
                        '\n' => out.push_str("\\n"),
                        '\r' => out.push_str("\\r"),
                        '\t' => out.push_str("\\t"),
                        '\u{8}' => out.push_str("\\b"),
                        '\u{c}' => out.push_str("\\f"),
                        c if (c as u32) < 0x20 || (c as u32) > 0x7e => {
                            let mut units = [0u16; 2];
                            for unit in c.encode_utf16(&mut units) {
                                out.push_str(&format!("\\u{unit:04x}"));
                            }
                        }
                        c => out.push(c),
                    }
                }
                out.push('"');
            } else {
                jsonio::push_json_str(out, s);
            }
        }
        Value::Array(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push_str(separators.0);
                }
                encode(out, item, separators, ascii);
            }
            out.push(']');
        }
        Value::Object(map) => {
            out.push('{');
            for (i, (key, item)) in map.iter().enumerate() {
                if i > 0 {
                    out.push_str(separators.0);
                }
                encode(out, &Value::str(key), separators, ascii);
                out.push_str(separators.1);
                encode(out, item, separators, ascii);
            }
            out.push('}');
        }
    }
}

// ------------------------------------------------------------ the counters

fn empty() -> Map {
    FIELDS.iter().map(|f| (f.to_string(), Value::Object(Map::new()))).collect()
}

/// A counter: a whole number above 0, no truth value (`page_usage._count`);
/// one beyond `i64` stays the largest.
fn count(value: &Value) -> Option<i64> {
    match value {
        Value::Int(n) if !n.starts_with('-') && n.len() > 18 => Some(n.parse::<i64>().unwrap_or(i64::MAX)),
        Value::Int(n) => n.parse::<i64>().ok().filter(|n| *n > 0),
        _ => None,
    }
}

/// A page id: `^[a-z_]{1,32}$` as Python's `re.match` reads it, a final line
/// break included.
fn page_like(page: &str) -> bool {
    let page = page.strip_suffix('\n').unwrap_or(page);
    (1..=32).contains(&page.len()) && page.bytes().all(|b| b.is_ascii_lowercase() || b == b'_')
}

/// A step of clicks: `0` to `10`, written as such.
fn step_like(key: &str) -> bool {
    !key.is_empty()
        && key.bytes().all(|b| b.is_ascii_digit())
        && key.parse::<i64>().is_ok_and(|n| (0..=MAX_CLICKS).contains(&n) && n.to_string() == key)
}

fn block<'a>(data: &'a Map, field: &str) -> &'a Map {
    static NONE: Map = Vec::new();
    match data.iter().find(|(k, _)| k == field).map(|(_, v)| v) {
        Some(Value::Object(map)) => map,
        _ => &NONE,
    }
}

fn block_mut<'a>(data: &'a mut Map, field: &str) -> &'a mut Map {
    if !data.iter().any(|(k, _)| k == field) {
        data.push((field.to_string(), Value::Object(Map::new())));
    }
    let slot = &mut data.iter_mut().find(|(k, _)| k == field).expect("just added").1;
    if !matches!(slot, Value::Object(_)) {
        *slot = Value::Object(Map::new());
    }
    match slot {
        Value::Object(map) => map,
        _ => unreachable!("an object"),
    }
}

fn number(map: &Map, key: &str) -> i64 {
    map.iter().find(|(k, _)| k == key).and_then(|(_, v)| count(v)).unwrap_or(0)
}

fn add(map: &mut Map, key: &str, n: i64) {
    let value = Value::int(number(map, key).saturating_add(n));
    jsonio::map_set(map, key, value);
}

/// Only the known form: ids, routes and numbers above 0; old page names on
/// their new page (`page_usage.clean`).
pub fn clean(raw: &Value) -> Map {
    let mut out = empty();
    let Value::Object(raw) = raw else { return out };
    for (page, n) in block(raw, "pages") {
        if let (true, Some(n)) = (page_like(page), count(n)) {
            jsonio::map_set(block_mut(&mut out, "pages"), page, Value::int(n));
        }
    }
    for field in ["entry", "clicks"] {
        let key_ok = |k: &str| if field == "entry" { ROUTES.contains(&k) } else { step_like(k) };
        for (page, inner) in block(raw, field) {
            let Value::Object(inner) = inner else { continue };
            if !page_like(page) {
                continue;
            }
            let mut kept = Map::new();
            for (key, n) in inner {
                if let (true, Some(n)) = (key_ok(key), count(n)) {
                    jsonio::map_set(&mut kept, key, Value::int(n));
                }
            }
            if !kept.is_empty() {
                jsonio::map_set(block_mut(&mut out, field), page, Value::Object(kept));
            }
        }
    }
    for (pair, n) in block(raw, "misses") {
        let parts: Vec<&str> = pair.split('>').collect();
        if let (true, Some(n)) =
            (parts.len() == 2 && parts.iter().all(|p| page_like(p)) && parts[0] != parts[1], count(n))
            && block(&out, "misses").len() < MAX_STORED_PAIRS
        {
            jsonio::map_set(block_mut(&mut out, "misses"), pair, Value::int(n));
        }
    }
    for (key, n) in block(raw, "actions") {
        if let (true, Some(n)) = (ACTIONS.contains(&key.as_str()), count(n)) {
            jsonio::map_set(block_mut(&mut out, "actions"), key, Value::int(n));
        }
    }
    merge_aliases(out)
}

fn new_page(page: &str) -> &str {
    ALIASES.iter().find(|(old, _)| *old == page).map_or(page, |(_, new)| new)
}

/// Counted under an old page name goes to the new page, added up (`page_usage._merge_aliases`).
fn merge_aliases(state: Map) -> Map {
    let mut out = empty();
    for (page, n) in block(&state, "pages") {
        add(block_mut(&mut out, "pages"), new_page(page), count(n).unwrap_or(0));
    }
    for field in ["entry", "clicks"] {
        for (page, inner) in block(&state, field) {
            let Value::Object(inner) = inner else { continue };
            let target = block_mut(block_mut(&mut out, field), new_page(page));
            for (key, n) in inner {
                add(target, key, count(n).unwrap_or(0));
            }
        }
    }
    for (pair, n) in block(&state, "misses") {
        let Some((first, second)) = pair.split_once('>') else { continue };
        let (first, second) = (new_page(first), new_page(second));
        if first != second {
            add(block_mut(&mut out, "misses"), &format!("{first}>{second}"), count(n).unwrap_or(0));
        }
    }
    let actions = block(&state, "actions").clone();
    *block_mut(&mut out, "actions") = actions;
    out
}

/// The fields of the report: pairs cut to the most frequent ones, every
/// counter at most [`MAX_COUNT`], sorted as 4.12 sorts (`page_usage.outgoing`).
pub fn outgoing(state: &Value) -> Map {
    let state = clean(state);
    let cap = |n: &Value| Value::Int(count(n).unwrap_or(0).min(MAX_COUNT).to_string());
    let sorted = |map: &Map| {
        let mut entries = map.clone();
        entries.sort_by(|a, b| a.0.cmp(&b.0));
        entries
    };
    let pages: Map = sorted(block(&state, "pages")).iter().map(|(p, n)| (p.clone(), cap(n))).collect();
    let nested = |field: &str, numeric: bool| -> Map {
        sorted(block(&state, field))
            .into_iter()
            .map(|(page, inner)| {
                let Value::Object(inner) = inner else { return (page, Value::Object(Map::new())) };
                let mut entries = inner.clone();
                if numeric {
                    entries.sort_by_key(|(k, _)| k.parse::<i64>().unwrap_or(0));
                } else {
                    entries.sort_by(|a, b| a.0.cmp(&b.0));
                }
                (page, Value::Object(entries.iter().map(|(k, n)| (k.clone(), cap(n))).collect()))
            })
            .collect()
    };
    let mut top = block(&state, "misses").clone();
    top.sort_by(|a, b| count(&b.1).cmp(&count(&a.1)).then(a.0.cmp(&b.0)));
    let misses: Map = top.iter().take(MAX_MISS_PAIRS).map(|(p, n)| (p.clone(), cap(n))).collect();
    let actions: Map = sorted(block(&state, "actions")).iter().map(|(k, n)| (k.clone(), cap(n))).collect();
    vec![
        ("pages".into(), Value::Object(pages)),
        ("entry".into(), Value::Object(nested("entry", false))),
        ("clicks".into(), Value::Object(nested("clicks", true))),
        ("misses".into(), Value::Object(misses)),
        ("actions".into(), Value::Object(actions)),
    ]
}

/// The way through the open main window (`page_usage._session`).
#[derive(Clone, Debug, Default)]
struct Session {
    page: Option<String>,
    since: f64,
    clicks: i64,
    clicks_at_entry: i64,
    misses: Vec<String>,
}

#[derive(Default)]
struct State {
    data: Option<Map>,
    session: Option<Session>,
    /// The counter file of before was dropped once; while counting is off nothing is asked again.
    dropped: bool,
    /// The day of the last report taken in this run, should the settings file refuse it.
    sent_day: Option<String>,
}

/// The counting of pages and actions (`page_usage`), for one data folder and
/// one version. Safe to share between the window and the background.
pub struct PageUsage {
    paths: Paths,
    version: String,
    started: Instant,
    state: Mutex<State>,
}

impl PageUsage {
    pub fn new(paths: Paths, version: &str) -> PageUsage {
        PageUsage { paths, version: version.to_string(), started: Instant::now(), state: Mutex::new(State::default()) }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Is counting on? Only while a report would go out (`page_usage.active`); on a copy of the data
    /// (the test mode) only to a receiver on this computer.
    pub fn active(&self) -> bool {
        would_send(&self.paths.host, &self.version)
            && target(&self.paths.host).is_some_and(|address| crate::testmode::usage_allowed(&self.paths, &address))
    }

    fn file(&self) -> String {
        self.paths.app_file(FILE)
    }

    fn load<'a>(&self, state: &'a mut State) -> &'a mut Map {
        if state.data.is_none() {
            let read = jsonio::read_json(&self.file()).map(|v| clean(&v)).unwrap_or_else(|_| empty());
            state.data = Some(read);
        }
        state.data.get_or_insert_with(empty)
    }

    /// Writes the counters as 4.12 does: compact, `ensure_ascii=False`; a
    /// failure stays silent, as there.
    fn save(&self, state: &State) {
        let Some(data) = &state.data else { return };
        let _ = jsonio::write_text_atomic(&self.file(), &file_text(data), Newline::Keep);
    }

    /// Counters dropped, in memory and on disk (`page_usage._drop`); the file only the first time,
    /// not again on every change of page.
    fn drop_all(&self, state: &mut State) {
        state.data = Some(empty());
        state.session = None;
        if !state.dropped {
            state.dropped = true;
            let _ = jsonio::remove_file(&self.file());
        }
    }

    /// Seconds of this counting's own clock.
    fn now(&self, clock: Option<f64>) -> f64 {
        clock.unwrap_or_else(|| self.started.elapsed().as_secs_f64())
    }

    /// A new main window: the way starts at no clicks (`page_usage.window_opened`).
    pub fn window_opened(&self) {
        let mut state = self.lock();
        if !self.active() {
            self.drop_all(&mut state);
            return;
        }
        state.session = Some(Session::default());
    }

    /// A group of the side bar was opened or closed by a click (`page_usage.group_toggled`).
    pub fn group_toggled(&self) {
        if let Some(session) = &mut self.lock().session {
            session.clicks += 1;
        }
    }

    /// The page open until now: a target or a miss (`page_usage._finish_page`).
    fn finish_page(&self, state: &mut State, now: f64) -> bool {
        let Some(session) = state.session.clone() else { return false };
        let Some(page) = session.page.clone() else { return false };
        if now - session.since < MISS_SECONDS {
            if let Some(session) = &mut state.session
                && !session.misses.contains(&page)
            {
                session.misses.push(page);
                let excess = session.misses.len().saturating_sub(MAX_MISSES_PER_TARGET);
                session.misses.drain(..excess);
            }
            return false;
        }
        let data = self.load(state);
        let steps = session.clicks_at_entry.min(MAX_CLICKS);
        add(block_mut(block_mut(data, "clicks"), &page), &steps.to_string(), 1);
        for miss in &session.misses {
            let pair = format!("{miss}>{page}");
            let misses = block_mut(data, "misses");
            if *miss != page && (misses.iter().any(|(k, _)| *k == pair) || misses.len() < MAX_STORED_PAIRS) {
                add(misses, &pair, 1);
            }
        }
        if let Some(session) = &mut state.session {
            session.misses.clear();
            session.clicks -= session.clicks_at_entry;
        }
        true
    }

    /// A page stands, opened by `route`; `None` is the program showing it anew,
    /// which counts nothing (`page_usage.page_opened`).
    pub fn page_opened(&self, page: &str, route: Option<&str>, clock: Option<f64>) {
        let Some(route) = route else { return };
        let mut state = self.lock();
        if !self.active() {
            self.drop_all(&mut state);
            return;
        }
        let known = PAGES.contains(&page);
        match &state.session {
            Some(session) if ROUTES.contains(&route) && known && session.page.as_deref() != Some(page) => {}
            _ => return,
        }
        let now = self.now(clock);
        self.finish_page(&mut state, now);
        if let Some(session) = &mut state.session
            && route != "start"
        {
            session.clicks += 1;
        }
        let data = self.load(&mut state);
        add(block_mut(data, "pages"), page, 1);
        add(block_mut(block_mut(data, "entry"), page), route, 1);
        if let Some(session) = &mut state.session {
            session.page = Some(page.to_string());
            session.since = now;
            session.clicks_at_entry = session.clicks;
        }
        self.save(&state);
    }

    /// An action of [`ACTIONS`] once, only how often (`page_usage.action`).
    pub fn action(&self, key: &str) {
        let mut state = self.lock();
        if !self.active() {
            self.drop_all(&mut state);
            return;
        }
        if !ACTIONS.contains(&key) {
            return;
        }
        add(block_mut(self.load(&mut state), "actions"), key, 1);
        self.save(&state);
    }

    /// The main window closes: the last page counts; misses without a following
    /// target fall away (`page_usage.window_closed`).
    pub fn window_closed(&self, clock: Option<f64>) {
        let mut state = self.lock();
        if state.session.is_none() {
            return;
        }
        let now = self.now(clock);
        if self.active() && self.finish_page(&mut state, now) {
            self.save(&state);
        }
        state.session = None;
    }

    /// A copy of the kept counters, not yet cut (`page_usage.stored`).
    pub fn stored(&self) -> Map {
        let mut state = self.lock();
        if !self.active() {
            return empty();
        }
        self.load(&mut state).clone()
    }

    /// After a report: exactly what it held is taken off; what came meanwhile
    /// stays for the next one (`page_usage.forget`).
    pub fn forget(&self, sent: &Map) {
        let sent = clean(&Value::Object(sent.clone()));
        let mut state = self.lock();
        let data = self.load(&mut state);
        for (page, n) in block(&sent, "pages") {
            take(block_mut(data, "pages"), page, count(n).unwrap_or(0));
        }
        for field in ["entry", "clicks"] {
            for (page, inner) in block(&sent, field) {
                let Value::Object(inner) = inner else { continue };
                let blocks = block_mut(data, field);
                let Some(slot) = blocks.iter_mut().find(|(k, _)| k == page) else { continue };
                if let Value::Object(kept) = &mut slot.1 {
                    for (key, n) in inner {
                        take(kept, key, count(n).unwrap_or(0));
                    }
                }
                let now_empty = matches!(&slot.1, Value::Object(kept) if kept.is_empty());
                if now_empty {
                    blocks.retain(|(k, _)| k != page);
                }
            }
        }
        for (pair, n) in block(&sent, "misses") {
            take(block_mut(data, "misses"), pair, count(n).unwrap_or(0));
        }
        for (key, n) in block(&sent, "actions") {
            take(block_mut(data, "actions"), key, count(n).unwrap_or(0));
        }
        if FIELDS.iter().any(|field| !block(data, field).is_empty()) {
            self.save(&state);
        } else {
            let _ = jsonio::remove_file(&self.file());
        }
    }
}

fn take(map: &mut Map, key: &str, n: i64) {
    let left = number(map, key) - n;
    if left > 0 {
        add(map, key, -n);
    } else {
        map.retain(|(k, _)| k != key);
    }
}

// ------------------------------------------------------------ sending

/// What a look for the daily report did (`usage_ping.send_if_due`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sent {
    /// Switched off: no network, or no target.
    Off,
    /// A version the receiver does not take.
    Version,
    /// Sent today already.
    Already,
    Sent,
    Failed,
}

/// Seconds until the next look: soon after a failure, else hourly (`usage_ping.wait_after`).
pub fn wait_after(sent: Sent) -> Duration {
    if sent == Sent::Failed { RETRY_AFTER } else { CHECK_EVERY }
}

/// The report in the background (`usage_ping.start`): at once, then hourly, ten minutes after a failed try. A
/// thread of its own; nothing starts where no report would go out (switched off, a version the receiver
/// does not take). The program that is "the one start" calls this for the player's own data.
pub fn spawn_ping(paths: Paths, usage: std::sync::Arc<PageUsage>) {
    if !usage.active() {
        return;
    }
    let _ = std::thread::Builder::new().name("nutzung".into()).spawn(move || {
        let Ok(transport) = crate::basetool::http::HttpTransport::new("", &paths.host) else { return };
        loop {
            let facts = facts(&paths, crate::language::current().code());
            let day = today_utc(std::time::SystemTime::now());
            std::thread::sleep(wait_after(send_if_due(&transport, &usage, &facts, &day)));
        }
    });
}

/// Sends the report when today's has not gone out yet; the day is noted and
/// the sent counters are taken off only after the receiver took it. As in
/// 4.12 the whole kept counters are taken off, also what the report cut
/// (counters above [`MAX_COUNT`], pairs beyond [`MAX_MISS_PAIRS`]). Unlike
/// 4.12 the day is also kept in memory: a settings file that refuses it
/// sends no second report the same day.
pub fn send_if_due(transport: &dyn Transport, usage: &PageUsage, facts: &Facts, day: &str) -> Sent {
    let paths = &usage.paths;
    // A copy of the data (the test mode) never reports to the real receiver: test runs would count as players.
    let Some(address) =
        target(&paths.host).filter(|a| !no_net(&paths.host) && crate::testmode::usage_allowed(paths, a))
    else {
        return Sent::Off;
    };
    if !version_ok(&usage.version) {
        return Sent::Version;
    }
    if paths.setting(LAST_SETTING).as_deref() == Some(day) || usage.lock().sent_day.as_deref() == Some(day) {
        return Sent::Already;
    }
    let state = usage.stored();
    let body = payload_text(&payload(&usage.version, facts, &state)).into_bytes();
    let request = Request {
        method: "POST".into(),
        url: address,
        headers: vec![
            ("Content-Type".into(), "application/json".into()),
            ("User-Agent".into(), format!("VerseKit/{}", usage.version)),
        ],
        body: Some(body),
    };
    match transport.send(&request) {
        Ok(answer) if matches!(answer.status, 200 | 204) => {
            paths.set_setting(LAST_SETTING, Value::str(day));
            usage.lock().sent_day = Some(day.to_string());
            usage.forget(&state);
            Sent::Sent
        }
        _ => Sent::Failed,
    }
}

#[cfg(test)]
mod tests;
