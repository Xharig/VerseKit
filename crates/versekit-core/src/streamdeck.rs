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

//! The contract between VerseKit and its Stream Deck plugin: the key file the plugin finds the interface by, the
//! actions of the plugin, the commands it may send and the pages it may open.
//!
//! The interface is HTTP/1.1 on `127.0.0.1` ([`crate::streamdeck_server`]):
//!
//! | Request | Answer |
//! |---|---|
//! | `GET /v1/zustand?seit=<rev>` | 200 with the state ([`STATE_FIELDS`]), 304 when its `rev` is still `seit` |
//! | `POST /v1/befehl` with `{"id": …}` | 200 `{"ok": true}`, or 400 with the refusal ([`Refusal`]) |
//! | `GET /v1/belegung` | 200 with the bindings of the game actions |
//!
//! Every request carries `Authorization: Bearer <schluessel>` from the key file. Fields a side does not know are
//! ignored, so either side may add one without raising [`PROTOCOL`].
//!
//! ⚠ The key is a secret: it is never written to a log, a message or an answer.

use std::io;

use serde_json::{Map, Value, json};

use crate::paths::Paths;

/// The version of the interface; both sides refuse another.
pub const PROTOCOL: u64 = 1;
/// The key file in the configuration folder ([`Paths::config_file`]).
pub const KEY_FILE: &str = "streamdeck.json";
/// Setting: the interface on or off (off unless the player switches it on).
pub const SETTING_ON: &str = "streamdeck_an";
/// The prefix of the plugin and of the ids of its actions.
pub const PLUGIN_UUID: &str = "com.xharig.versekit";

pub const PATH_STATE: &str = "/v1/zustand";
pub const PATH_COMMAND: &str = "/v1/befehl";
pub const PATH_BINDINGS: &str = "/v1/belegung";
/// What the page of VerseKit shows about the connection (asked by the main window, not by the plugin).
pub const PATH_REPORT: &str = "/v1/verbindung";

/// The most bytes a request body may have: room for a report of [`MAX_REPORTED_ACTIONS`] names of the longest form.
pub const MAX_BODY: usize = 64 * 1024;

/// The top-level fields of the state, in the order they are written.
pub const STATE_FIELDS: &[&str] = &[
    "version",
    "rev",
    "sprache",
    "farben",
    "overlay",
    "hauptfenster",
    "anti_afk",
    "raffinerie",
    "contested",
    "konto",
    "belegung_stand",
    "spielaktion",
    FIELD_DEVICE_PAGE,
    FIELD_DIALS,
    FIELD_CONTRACTS,
    FIELD_STATUS,
    FIELD_SHIP,
    FIELD_HEALTH,
];

/// The field of the state with the health of the player from the telemetry of the game (Star Citizen 5.0,
/// `ActorVitals.health`): `{"aktuell": n, "max": n}`, or `null` without it.
pub const FIELD_HEALTH: &str = "gesundheit";

/// The field [`FIELD_HEALTH`] for a health of `current` out of `max`.
pub fn health_value(health: Option<(f64, f64)>) -> Value {
    health.map_or(Value::Null, |(current, max)| json!({"aktuell": current, "max": max}))
}

/// The health a state carries, in per cent (0 to 100, rounded); `None` without it or without a maximum.
pub fn health_of(state: &State) -> Option<u8> {
    let field = state.get(FIELD_HEALTH)?;
    let (current, max) = (field["aktuell"].as_f64()?, field["max"].as_f64()?);
    (max > 0.0).then(|| (current / max * 100.0).clamp(0.0, 100.0).round() as u8)
}

/// The short names of the game actions on the keys of the profiles, `[de, en]` by action.
const SHORT_NAMES: &str = include_str!("../../../streamdeck-plugin/kurznamen.json");

/// The short name of the game action `action` (also as `actionmap.action`) in `lang`, as the profiles show it;
/// `None` for an action without one.
pub fn short_name(action: &str, lang: crate::language::Lang) -> Option<&'static str> {
    static TABLE: std::sync::OnceLock<std::collections::HashMap<String, [String; 2]>> = std::sync::OnceLock::new();
    let table = TABLE.get_or_init(|| {
        let source: Value = serde_json::from_str(SHORT_NAMES).expect("kurznamen.json");
        source["aktionen"]
            .as_object()
            .map(|entries| {
                entries
                    .iter()
                    .filter_map(|(action, names)| {
                        Some((action.clone(), [names[0].as_str()?.to_string(), names[1].as_str()?.to_string()]))
                    })
                    .collect()
            })
            .unwrap_or_default()
    });
    let plain = action.split_once('.').map_or(action, |(_, plain)| plain);
    let names = table.get(plain)?;
    Some(if lang == crate::language::Lang::En { names[1].as_str() } else { names[0].as_str() })
}

/// The name of a game action as the deck and its page show it: the name of the game where it has one in `lang`
/// (`own_text`), else the short name of the profiles, else what the game has (English or technical); on one line.
pub fn game_action_name(action: &str, game_name: &str, own_text: bool, lang: crate::language::Lang) -> String {
    let name = if own_text { None } else { short_name(action, lang) };
    crate::controls::one_line(name.unwrap_or(game_name))
}

/// The game actions that are dangerous by themselves (eject, emergency exit, self destruct, all power off): a key
/// with one of them sends it only after it was held, unless its profile says otherwise.
pub const DANGEROUS_ACTIONS: [&str; 4] = ["v_eject", "v_emergency_exit", "v_self_destruct", "v_power_toggle"];

/// Is the game action `action` (also as `actionmap.action`) dangerous by itself?
pub fn dangerous_by_itself(action: &str) -> bool {
    let plain = action.split_once('.').map_or(action, |(_, plain)| plain);
    DANGEROUS_ACTIONS.contains(&plain)
}

/// The field of the state with the last ship of the running session: `{"name": text}`, or `null` without one.
pub const FIELD_SHIP: &str = "schiff";
/// The page the ship dial opens.
pub const PAGE_SHIPS: &str = "hf_st_schiffe";

/// The field [`FIELD_SHIP`] for the readable name of a ship.
pub fn ship_value(name: Option<&str>) -> Value {
    name.map_or(Value::Null, |n| json!({ "name": n }))
}

/// The name of the ship a state carries; `None` without one.
pub fn ship_of(state: &State) -> Option<String> {
    state.get(FIELD_SHIP)?["name"].as_str().filter(|n| !n.is_empty()).map(String::from)
}

/// The field of the state that says whether all is ready: `{"spiel_laeuft": bool, "log_ok": bool | null,
/// "tasten_fehlen": n}` (`log_ok` is `null` while the watch of the log has not looked yet).
pub const FIELD_STATUS: &str = "status";

/// Whether all is ready, as the state carries it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Readiness {
    pub game_runs: bool,
    /// `None` while nobody looked yet.
    pub log_found: Option<bool>,
    /// Game actions on the deck without a key on the keyboard.
    pub keys_missing: usize,
}

impl Readiness {
    pub fn value(self) -> Value {
        json!({"spiel_laeuft": self.game_runs, "log_ok": self.log_found, "tasten_fehlen": self.keys_missing})
    }

    /// The readiness a state carries; `None` without it.
    pub fn of(state: &State) -> Option<Readiness> {
        let field = state.get(FIELD_STATUS)?;
        Some(Readiness {
            game_runs: field["spiel_laeuft"].as_bool()?,
            log_found: field["log_ok"].as_bool(),
            keys_missing: field["tasten_fehlen"].as_u64().unwrap_or(0) as usize,
        })
    }
}

/// A time left as a clock reads it: `12:05` (minutes and seconds), `1:02:05` from an hour on.
pub fn clock(seconds: u64) -> String {
    let (hours, minutes, rest) = (seconds / 3600, (seconds % 3600) / 60, seconds % 60);
    if hours > 0 { format!("{hours}:{minutes:02}:{rest:02}") } else { format!("{minutes:02}:{rest:02}") }
}

/// The field of the state with the running contracts, in the order of the bar of the overlay: each
/// `{"titel", "schritt", "erledigt", "von"}` (the objective now, or `null`; objectives done and in all, or `null`).
pub const FIELD_CONTRACTS: &str = "auftraege";

/// One running contract as the objective dial shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContractCard {
    pub title: String,
    /// The first open objective.
    pub objective: Option<String>,
    /// Objectives done and in all.
    pub steps: Option<(usize, usize)>,
}

impl ContractCard {
    /// The number of the objective now among all: one past the ones done, at most all.
    pub fn step_now(&self) -> Option<(usize, usize)> {
        self.steps.map(|(done, all)| ((done + 1).min(all), all))
    }
}

/// The field [`FIELD_CONTRACTS`] of the running contracts of the bar.
pub fn contracts_value(lines: &[crate::live_contracts::Line]) -> Value {
    Value::Array(
        lines
            .iter()
            .map(|line| {
                json!({
                    "titel": line.title,
                    "schritt": line.goals.first(),
                    "erledigt": line.steps.map(|(done, _)| done),
                    "von": line.steps.map(|(_, all)| all),
                })
            })
            .collect(),
    )
}

/// The running contracts a state carries; empty without them.
pub fn contracts_of(state: &State) -> Vec<ContractCard> {
    state
        .get(FIELD_CONTRACTS)
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .filter_map(|item| {
                    let title = item["titel"].as_str()?.to_string();
                    let objective = item["schritt"].as_str().filter(|s| !s.is_empty()).map(String::from);
                    let steps = match (item["erledigt"].as_u64(), item["von"].as_u64()) {
                        (Some(done), Some(all)) if all > 0 => Some((done as usize, all as usize)),
                        _ => None,
                    };
                    Some(ContractCard { title, objective, steps })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The field of the state that says how the dials are drawn: `{"kompakt": bool}`.
pub const FIELD_DIALS: &str = "regler";
/// Setting: the dials without their name, the value or the symbol larger (off: with the name).
pub const SETTING_DIAL_COMPACT: &str = "streamdeck_regler_kompakt";

// ------------------------------------------------------------ key file

/// What the plugin needs to reach the interface.
#[derive(Clone, PartialEq, Eq)]
pub struct KeyFile {
    pub port: u16,
    /// 64 lower-case hex characters.
    pub key: String,
    /// The process that serves the interface.
    pub pid: u32,
}

impl std::fmt::Debug for KeyFile {
    // The key never shows in a debug print.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KeyFile").field("port", &self.port).field("key", &"…").field("pid", &self.pid).finish()
    }
}

impl KeyFile {
    /// The file as text.
    pub fn to_text(&self) -> String {
        let value = json!({"version": PROTOCOL, "port": self.port, "schluessel": self.key, "pid": self.pid});
        serde_json::to_string_pretty(&value).unwrap_or_default()
    }

    /// The file read back; `None` for another version, a port 0, a key of another form.
    pub fn parse(text: &str) -> Option<KeyFile> {
        let value: Value = serde_json::from_str(text.trim_start_matches('\u{feff}')).ok()?;
        if value.get("version")?.as_u64()? != PROTOCOL {
            return None;
        }
        let port = u16::try_from(value.get("port")?.as_u64()?).ok().filter(|p| *p > 0)?;
        let key = value.get("schluessel")?.as_str()?.to_string();
        if !valid_key(&key) {
            return None;
        }
        let pid = value.get("pid").and_then(Value::as_u64).and_then(|p| u32::try_from(p).ok()).unwrap_or(0);
        Some(KeyFile { port, key, pid })
    }
}

/// A new key: 32 random bytes as 64 lower-case hex characters.
pub fn new_key() -> Option<String> {
    crate::hangar_inbox::new_token()
}

/// 64 lower-case hex characters.
pub fn valid_key(key: &str) -> bool {
    crate::hangar_inbox::valid_token(key)
}

/// Compares two secrets without stopping at the first difference.
pub fn same_secret(a: &str, b: &str) -> bool {
    crate::hangar_inbox::same_secret(a, b)
}

/// Where the key file lies.
pub fn key_file_path(paths: &Paths) -> String {
    paths.config_file(KEY_FILE)
}

/// Writes the key file (whole or not at all).
///
/// # Errors
///
/// The error of the writing.
pub fn write_key_file(paths: &Paths, file: &KeyFile) -> io::Result<()> {
    let target = key_file_path(paths);
    if let Some(folder) = std::path::Path::new(&target).parent() {
        crate::jsonio::make_dirs(&folder.to_string_lossy())?;
    }
    crate::jsonio::write_text_atomic(&target, &file.to_text(), crate::jsonio::Newline::Keep)
}

/// Removes the key file, if it is the one of `pid` (another process may have written its own since).
pub fn remove_key_file(paths: &Paths, pid: u32) {
    let target = key_file_path(paths);
    let ours = std::fs::read_to_string(&target).ok().and_then(|text| KeyFile::parse(&text)).is_some_and(|f| f.pid == pid);
    if ours {
        let _ = crate::jsonio::remove_file(&target);
    }
}

/// Reads the key file; `None` when there is none or it is not valid.
pub fn read_key_file(paths: &Paths) -> Option<KeyFile> {
    std::fs::read_to_string(key_file_path(paths)).ok().and_then(|text| KeyFile::parse(&text))
}

/// The file the plugin is installed from, beside the program of VerseKit.
pub const PLUGIN_FILE: &str = "VerseKit.streamDeckPlugin";
/// Where the Stream Deck program is offered.
pub const ELGATO_URL: &str = "https://www.elgato.com/downloads";

/// Is the Stream Deck program of Elgato installed? Looked for in its folder under the programs.
pub fn app_installed(host: &crate::paths::Host) -> bool {
    ["ProgramFiles", "ProgramW6432"]
        .iter()
        .filter_map(|name| host.var_set(name))
        .any(|base| std::path::Path::new(base).join("Elgato").join("StreamDeck").join("StreamDeck.exe").is_file())
}

/// The plugin file beside the running program; `None` where there is none.
pub fn plugin_file() -> Option<String> {
    let exe = std::env::current_exe().ok()?;
    let file = exe.parent()?.join(PLUGIN_FILE);
    file.is_file().then(|| file.to_string_lossy().into_owned())
}

/// The setting that remembers the version of the plugin file VerseKit opened once by itself after an update, so it
/// does not open it again (the player may have declined it in the Stream Deck program).
pub const SETTING_PLUGIN_OFFERED: &str = "streamdeck_plugin_angeboten";

/// The version a plugin file names in its manifest (`Version`); `None` for a file that is none.
pub fn plugin_file_version(bytes: &[u8]) -> Option<String> {
    let entries = crate::zip::entries(bytes).ok()?;
    let manifest = entries.iter().find(|e| {
        let mut parts = e.name.split('/');
        matches!((parts.next(), parts.next(), parts.next()), (Some(folder), Some("manifest.json"), None) if folder.ends_with(".sdPlugin"))
    })?;
    let value: Value = serde_json::from_slice(&crate::zip::read(bytes, manifest).ok()?).ok()?;
    value["Version"].as_str().filter(|v| plausible_version(v)).map(String::from)
}

/// A version as the manifest writes it: one to four numbers with dots, at most 40 characters.
pub fn plausible_version(text: &str) -> bool {
    !text.is_empty()
        && text.len() <= 40
        && text.split('.').count() <= 4
        && text.split('.').all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()))
}

/// Is version `installed` older than `shipped`? Compared number by number, a missing number counts as 0; an
/// implausible version is never older.
pub fn version_older(installed: &str, shipped: &str) -> bool {
    if !plausible_version(installed) || !plausible_version(shipped) {
        return false;
    }
    let numbers = |v: &str| -> Vec<u64> {
        let mut n: Vec<u64> = v.split('.').map(|p| p.parse().unwrap_or(u64::MAX)).collect();
        n.resize(4, 0);
        n
    };
    numbers(installed) < numbers(shipped)
}

// ------------------------------------------------------------ main window

/// The main window as the key of the Stream Deck shows it: every press does what the key shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MainWindow {
    /// Not open: a press opens it and brings it to the front.
    Closed,
    /// Open, but another window is in front: a press brings it to the front.
    Behind,
    /// Open and in front: a press closes it.
    Front,
}

/// What a press on the key of the main window does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MainPress {
    Open,
    ToFront,
    Close,
}

impl MainWindow {
    /// The window from whether it is there and shown, and whether it is the window in front.
    pub fn of(shown: bool, in_front: bool) -> MainWindow {
        match (shown, in_front) {
            (false, _) => MainWindow::Closed,
            (true, false) => MainWindow::Behind,
            (true, true) => MainWindow::Front,
        }
    }

    /// The word in the state (`hauptfenster`).
    pub fn id(self) -> &'static str {
        match self {
            MainWindow::Closed => "zu",
            MainWindow::Behind => "hinten",
            MainWindow::Front => "offen",
        }
    }

    pub fn from_id(id: &str) -> Option<MainWindow> {
        [MainWindow::Closed, MainWindow::Behind, MainWindow::Front].into_iter().find(|w| w.id() == id)
    }

    pub fn press(self) -> MainPress {
        match self {
            MainWindow::Closed => MainPress::Open,
            MainWindow::Behind => MainPress::ToFront,
            MainWindow::Front => MainPress::Close,
        }
    }
}

// ------------------------------------------------------------ actions

/// An action of the plugin: one kind of key on the Stream Deck.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Action {
    Overlay,
    Main,
    RefineryScan,
    Blueprint,
    Stock,
    UnitStock,
    Search,
    Picture,
    AntiAfk,
    Refinery,
    Page,
    Contested,
    Wallet,
    GameAction,
    /// An emote of the game: a tap goes back to a page afterwards, a hold stays.
    Emote,
    /// Opens the folder of the `actionmaps.xml`.
    BindingsFolder,
    /// The mining (signature) scanner on and off, as its hotkey and the eye of the bar; shows its state.
    MiningScanner,
    /// Clicks through the overlay into the game on and off, as the lock of the bar; shows its state.
    ClickThrough,
    /// The sounds of the overlay on and off; shows their state.
    Sounds,
    /// A dial that shows the running contracts: turning goes through them, a press opens the contract log.
    Objective,
    /// A dial that shows whether all is ready: the game runs, the log is found, the keys of the deck are bound; a
    /// press opens the page of the keys.
    Status,
    /// A dial that shows the last ship of the running session; a press opens the statistics of the ships.
    Ship,
    /// A key or a dial that shows the health of the player from the telemetry of the game; a press does nothing.
    Health,
}

impl Action {
    pub const ALL: [Action; 23] = [
        Action::Overlay,
        Action::Main,
        Action::RefineryScan,
        Action::Blueprint,
        Action::Stock,
        Action::UnitStock,
        Action::Search,
        Action::Picture,
        Action::AntiAfk,
        Action::Refinery,
        Action::Page,
        Action::Contested,
        Action::Wallet,
        Action::GameAction,
        Action::Emote,
        Action::BindingsFolder,
        Action::MiningScanner,
        Action::ClickThrough,
        Action::Sounds,
        Action::Objective,
        Action::Status,
        Action::Ship,
        Action::Health,
    ];

    /// The id in the commands and in the manifest.
    pub fn id(self) -> &'static str {
        match self {
            Action::Overlay => "overlay",
            Action::Main => "haupt",
            Action::RefineryScan => "scan",
            Action::Blueprint => "bauplan",
            Action::Stock => "lager",
            Action::UnitStock => "staffellager",
            Action::Search => "suche",
            Action::Picture => "bild",
            Action::AntiAfk => "anti_afk",
            Action::Refinery => "raffinerie",
            Action::Page => "seite",
            Action::Contested => "contested",
            Action::Wallet => "konto",
            Action::GameAction => "spielaktion",
            Action::Emote => "emote",
            Action::BindingsFolder => "belegungsordner",
            Action::MiningScanner => "mining_scanner",
            Action::ClickThrough => "durchklick",
            Action::Sounds => "toene",
            Action::Objective => "auftrag",
            Action::Status => "status",
            Action::Ship => "schiff",
            Action::Health => "gesundheit",
        }
    }

    pub fn from_id(id: &str) -> Option<Action> {
        Action::ALL.into_iter().find(|a| a.id() == id)
    }

    /// The UUID of the action in the manifest (`com.xharig.versekit.<id>`, lower case, no underscore).
    pub fn uuid(self) -> String {
        format!("{PLUGIN_UUID}.{}", self.id().replace('_', "-"))
    }

    pub fn from_uuid(uuid: &str) -> Option<Action> {
        Action::ALL.into_iter().find(|a| a.uuid() == uuid)
    }

    /// The Lucide symbol on the key.
    pub fn icon(self) -> &'static str {
        match self {
            Action::Overlay => "layers",
            // The symbols of the keys of the profiles.
            Action::Main => "layout-dashboard",
            Action::RefineryScan => "scan-text",
            Action::Blueprint => "file-scan",
            Action::Stock => "package",
            Action::UnitStock => "boxes",
            Action::Search => "user-round-search",
            Action::Picture => "camera",
            Action::AntiAfk => "coffee",
            Action::Refinery => "flask-conical",
            Action::Page => "panel-left",
            Action::Contested => "timer",
            Action::Wallet => "wallet",
            Action::GameAction => "keyboard",
            Action::Emote => "hand",
            Action::BindingsFolder => "folder-open",
            Action::MiningScanner => "pickaxe",
            Action::ClickThrough => "lock",
            Action::Sounds => "volume-2",
            Action::Objective => "clipboard-list",
            Action::Status => "activity",
            Action::Ship => "rocket",
            Action::Health => "heart-pulse",
        }
    }

    /// The text key of the name of the action.
    pub fn label_key(self) -> String {
        format!("sd_a_{}", self.id())
    }

    /// The text key of the tooltip of the action in the Stream Deck program.
    pub fn tooltip_key(self) -> String {
        format!("sd_t_{}", self.id())
    }
}

// ------------------------------------------------------------ pages

/// The pages a key may open: the page above the menu and those of the menu, in the order of the menu.
/// A test of the user interface keeps this list equal to its menu.
pub const PAGES: &[&str] = &[
    "hf_notizen",
    "hf_liste",
    "hf_auftragslog",
    "hf_hangar",
    "hf_meinlager",
    "hf_staffellager",
    "s_ol_seite",
    "hf_herstellung",
    "hf_bergbau",
    "hf_bergung",
    "hf_laeden",
    "hf_verkauf",
    "hf_routen",
    "hf_finanzen",
    "hf_bank",
    "hf_bedarf",
    "s_oa_seite",
    "hf_einkaufsliste",
    "hf_contested",
    "hf_st_uebersicht",
    "hf_st_schiffe",
    "hf_st_auftraege",
    "hf_st_quantum",
    "hf_st_stabil",
    "hf_st_flugbuch",
    "hf_allgemein",
    "hf_overlay",
    "hf_streamdeck",
    "hf_darstellung",
    "hf_ordner",
    "hf_module",
    "hf_startprogramme",
    "k_basetool_verbindung",
    "s_os_seite",
    "hf_sichern",
    "hf_spiel",
    "hf_uebersetzung",
    "hf_grafik",
    "hf_blickwinkel",
    "hf_joysticks",
    "hf_achsen",
    "hf_ueber",
    "hf_patchaenderungen",
    "hf_serverstatus",
    "hf_handlesuche",
    "hf_danke",
];

/// The page of the contested zones (the key of the hangar cycle opens it).
pub const PAGE_CONTESTED: &str = "hf_contested";
/// The page of the finances (the key of the balance opens it).
pub const PAGE_FINANCES: &str = "hf_finanzen";
/// The page of the keys and the Stream Deck (a key without a binding opens it, where the missing keys are put in).
pub const PAGE_KEYS: &str = "hf_streamdeck";
/// The page of the controls (the hint of a game action opens it).
pub const PAGE_CONTROLS: &str = "hf_joysticks";

pub fn page_allowed(page: &str) -> bool {
    PAGES.contains(&page)
}

// ------------------------------------------------------------ page of the device

/// Setting: the plugin switches the page of the device when the game log says the player left the seat (on unless
/// the player switches it off).
pub const SETTING_AUTO_PAGE: &str = "streamdeck_seite_auto";
/// The field of the state that asks the plugin for a page of the device: `{"nr": n, "seite": id}`, `nr` going up
/// with every wish, so the plugin switches once per wish and never again for the same one.
pub const FIELD_DEVICE_PAGE: &str = "geraeteseite";

/// A page of the shipped profiles the plugin switches to by itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DevicePage {
    OnFoot,
}

impl DevicePage {
    pub const ALL: [DevicePage; 1] = [DevicePage::OnFoot];

    /// The word in the state.
    pub fn id(self) -> &'static str {
        match self {
            DevicePage::OnFoot => "zu_fuss",
        }
    }

    pub fn from_id(id: &str) -> Option<DevicePage> {
        DevicePage::ALL.into_iter().find(|p| p.id() == id)
    }

    /// The names the page has in the shipped profiles, German and English. A page whose name begins with one of
    /// them is this page (`Zu Fuß 1` on a device with several pages of one kind).
    pub fn names(self) -> &'static [&'static str] {
        match self {
            DevicePage::OnFoot => &["Zu Fuß", "On Foot"],
        }
    }

    /// The number of the first page of this kind among the names of the pages of a profile, in their order.
    pub fn number_in(self, pages: &[String]) -> Option<usize> {
        pages.iter().position(|name| self.names().iter().any(|n| name.trim().starts_with(n)))
    }
}

/// The wish of the newest line of `text` that asks for a page of the device, with its stamp (seconds, UTC; `None`
/// for a line without one): the player released the control of a ship (left the seat of the pilot) → on foot.
/// Other lines ask for nothing.
pub fn device_page_in(text: &str) -> Option<(DevicePage, Option<i64>)> {
    text.split(['\n', '\r']).rev().find_map(|line| {
        let released = line.contains("releasing control token") && crate::play_stats::CONTROL.is_match(line);
        released.then(|| {
            let at = crate::logtime::seconds(crate::logtime::time_of(line)).filter(|s| *s != 0);
            (DevicePage::OnFoot, at)
        })
    })
}

/// The field [`FIELD_DEVICE_PAGE`] of the state: the newest wish and its number; `null` before the first.
pub fn device_page_value(wish: Option<(u64, DevicePage)>) -> Value {
    wish.map_or(Value::Null, |(nr, page)| json!({"nr": nr, "seite": page.id()}))
}

/// The wish a state carries: its number and the page; `None` without one or with a page this side does not know.
pub fn device_page_of(state: &State) -> Option<(u64, DevicePage)> {
    let field = state.get(FIELD_DEVICE_PAGE)?;
    Some((field["nr"].as_u64()?, DevicePage::from_id(field["seite"].as_str()?)?))
}

// ------------------------------------------------------------ commands

/// Pressed or released: a game action follows the key of the Stream Deck.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Down,
    Up,
}

/// A command of the plugin.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    /// The overlay shown or hidden, as its hotkey.
    Overlay,
    /// The main window: started, brought to the front, or closed when it is in front.
    Main,
    RefineryScan,
    Blueprint,
    Stock,
    UnitStock,
    Search,
    Picture,
    /// Anti-AFK switched over.
    AntiAfk,
    /// The mining scanner switched over, the way of its hotkey.
    MiningScanner,
    /// Clicking through the overlay switched over, as the lock of the bar.
    ClickThrough,
    /// The sounds of the overlay switched over.
    Sounds,
    /// A page of the main window.
    Page(String),
    /// A game action pressed or released, by its name in the `actionmaps.xml`.
    GameAction { action: String, phase: Phase },
    /// The game actions on the keys of the plugin, for the page that adds missing keys.
    GameActionsInUse(Vec<String>),
    /// The plugin says that the device has no profile of its own.
    DeviceHint(Option<String>),
    /// The folder of the `actionmaps.xml` opened in the file manager.
    BindingsFolder,
    /// The plugin says its version, so VerseKit can tell an older one from the plugin it brings.
    PluginVersion(String),
}

impl Command {
    /// The action whose limit of commands per second counts this command.
    pub fn limit_key(&self) -> String {
        match self {
            Command::Overlay => "overlay".into(),
            Command::Main => "haupt".into(),
            Command::RefineryScan => "scan".into(),
            Command::Blueprint => "bauplan".into(),
            Command::Stock => "lager".into(),
            Command::UnitStock => "staffellager".into(),
            Command::Search => "suche".into(),
            Command::Picture => "bild".into(),
            Command::AntiAfk => "anti_afk".into(),
            Command::MiningScanner => "mining_scanner".into(),
            Command::ClickThrough => "durchklick".into(),
            Command::Sounds => "toene".into(),
            Command::Page(_) => "seite".into(),
            // A press and its release are one press: only the press counts.
            Command::GameAction { action, phase: Phase::Down } => format!("spielaktion:{action}"),
            Command::GameAction { phase: Phase::Up, .. } => String::new(),
            Command::GameActionsInUse(_) => "spielaktionen_melden".into(),
            Command::DeviceHint(_) => "geraet".into(),
            Command::BindingsFolder => "belegungsordner".into(),
            Command::PluginVersion(_) => "plugin_version".into(),
        }
    }
}

/// Why a command or a request was refused: the code in the answer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// Not JSON, or not an object.
    Json,
    /// No `id`, or one that is no command.
    Id,
    /// A page that may not be opened.
    Page,
    /// A game action name that is not one of the game, or no name.
    GameAction,
    /// No or another `phase`.
    Phase,
    /// The list of game actions is not a list of names, or too long.
    List,
}

impl Refusal {
    pub fn code(self) -> &'static str {
        match self {
            Refusal::Json => "json",
            Refusal::Id => "id",
            Refusal::Page => "seite",
            Refusal::GameAction => "aktion",
            Refusal::Phase => "phase",
            Refusal::List => "liste",
        }
    }
}

/// The most game actions one report may name.
pub const MAX_REPORTED_ACTIONS: usize = 512;

/// A name of a game action as the `actionmaps.xml` writes it: letters, digits, `_`, `-`, `.`, at most 96.
pub fn plausible_action_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 96
        && name.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.'))
}

/// Reads a command body. The names of game actions are only checked for their form here; whether the game knows
/// them decides the server.
///
/// # Errors
///
/// The [`Refusal`].
pub fn parse_command(body: &[u8]) -> Result<Command, Refusal> {
    let value: Value = serde_json::from_slice(body).map_err(|_| Refusal::Json)?;
    let Some(map) = value.as_object() else { return Err(Refusal::Json) };
    let id = map.get("id").and_then(Value::as_str).ok_or(Refusal::Id)?;
    let command = match id {
        "overlay" => Command::Overlay,
        "haupt" => Command::Main,
        "scan" => Command::RefineryScan,
        "bauplan" => Command::Blueprint,
        "lager" => Command::Stock,
        "staffellager" => Command::UnitStock,
        "suche" => Command::Search,
        "bild" => Command::Picture,
        "anti_afk" => Command::AntiAfk,
        "mining_scanner" => Command::MiningScanner,
        "durchklick" => Command::ClickThrough,
        "toene" => Command::Sounds,
        "belegungsordner" => Command::BindingsFolder,
        "seite" => {
            let page = map.get("seite").and_then(Value::as_str).ok_or(Refusal::Page)?;
            if !page_allowed(page) {
                return Err(Refusal::Page);
            }
            Command::Page(page.to_string())
        }
        "contested" => Command::Page(PAGE_CONTESTED.into()),
        "konto" => Command::Page(PAGE_FINANCES.into()),
        "spielaktion" => {
            let action = map.get("aktion").and_then(Value::as_str).ok_or(Refusal::GameAction)?;
            if !plausible_action_name(action) {
                return Err(Refusal::GameAction);
            }
            let phase = match map.get("phase").and_then(Value::as_str) {
                Some("runter") => Phase::Down,
                Some("hoch") => Phase::Up,
                _ => return Err(Refusal::Phase),
            };
            Command::GameAction { action: action.to_string(), phase }
        }
        "spielaktionen_melden" => {
            let list = map.get("aktionen").and_then(Value::as_array).ok_or(Refusal::List)?;
            if list.len() > MAX_REPORTED_ACTIONS {
                return Err(Refusal::List);
            }
            let mut names = Vec::new();
            for item in list {
                let name = item.as_str().filter(|n| plausible_action_name(n)).ok_or(Refusal::List)?;
                if !names.iter().any(|n: &String| n == name) {
                    names.push(name.to_string());
                }
            }
            Command::GameActionsInUse(names)
        }
        "plugin_version" => {
            let version = map.get("version").and_then(Value::as_str).filter(|v| plausible_version(v)).ok_or(Refusal::Json)?;
            Command::PluginVersion(version.to_string())
        }
        "geraet" => {
            let hint = match map.get("hinweis") {
                None | Some(Value::Null) => None,
                Some(Value::String(text)) if text.len() <= 64 => Some(text.clone()),
                _ => return Err(Refusal::Json),
            };
            Command::DeviceHint(hint)
        }
        _ => return Err(Refusal::Id),
    };
    Ok(command)
}

/// A command as the plugin writes it.
pub fn command_body(command: &Command) -> String {
    let value = match command {
        Command::Page(page) => json!({"id": "seite", "seite": page}),
        Command::GameAction { action, phase } => json!({
            "id": "spielaktion",
            "aktion": action,
            "phase": if *phase == Phase::Down { "runter" } else { "hoch" },
        }),
        Command::GameActionsInUse(names) => json!({"id": "spielaktionen_melden", "aktionen": names}),
        Command::DeviceHint(hint) => json!({"id": "geraet", "hinweis": hint}),
        Command::PluginVersion(version) => json!({"id": "plugin_version", "version": version}),
        other => json!({"id": other.limit_key()}),
    };
    value.to_string()
}

/// The answer to a refused command.
pub fn refusal_body(refusal: Refusal) -> String {
    json!({"ok": false, "code": refusal.code()}).to_string()
}

// ------------------------------------------------------------ state

/// The state as the plugin reads it: only the fields it knows, the rest kept for later versions.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct State {
    pub rev: u64,
    pub fields: Map<String, Value>,
}

impl State {
    /// Reads a state answer; `None` for another version.
    pub fn parse(text: &str) -> Option<State> {
        let value: Value = serde_json::from_str(text).ok()?;
        let map = value.as_object()?.clone();
        if map.get("version").and_then(Value::as_u64) != Some(PROTOCOL) {
            return None;
        }
        let rev = map.get("rev").and_then(Value::as_u64).unwrap_or(0);
        Some(State { rev, fields: map })
    }

    pub fn get(&self, field: &str) -> Option<&Value> {
        self.fields.get(field).filter(|v| !v.is_null())
    }

    /// The language of VerseKit (`de` or `en`); German where it is missing.
    pub fn language(&self) -> crate::language::Lang {
        self.get("sprache").and_then(Value::as_str).and_then(crate::language::Lang::from_code).unwrap_or(crate::language::Lang::De)
    }

    pub fn flag(&self, field: &str) -> Option<bool> {
        self.get(field).and_then(Value::as_bool)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    #[test]
    fn the_key_file_goes_out_and_comes_back_and_refuses_what_it_is_not() {
        let file = KeyFile { port: 51234, key: KEY.into(), pid: 77 };
        assert_eq!(KeyFile::parse(&file.to_text()), Some(file.clone()));
        assert!(!format!("{file:?}").contains(KEY), "the key never in a debug print");
        let other = file.to_text().replace("\"version\": 1", "\"version\": 2");
        assert_eq!(KeyFile::parse(&other), None, "another version");
        assert_eq!(KeyFile::parse(&file.to_text().replace("51234", "0")), None, "port 0");
        assert_eq!(KeyFile::parse(&file.to_text().replace(KEY, "abc")), None, "short key");
        assert_eq!(KeyFile::parse(&file.to_text().replace(KEY, &KEY.to_uppercase())), None, "upper case");
        assert_eq!(KeyFile::parse("kein json"), None);
        let with_bom = format!("\u{feff}{}", file.to_text());
        assert_eq!(KeyFile::parse(&with_bom), Some(file), "a byte order mark is allowed");
    }

    #[test]
    fn a_new_key_is_valid_and_another_each_time() {
        let a = new_key().expect("random");
        let b = new_key().expect("random");
        assert!(valid_key(&a) && valid_key(&b));
        assert_ne!(a, b);
        assert!(same_secret(&a, &a.clone()));
        assert!(!same_secret(&a, &b));
        assert!(!same_secret(&a, &a[..63]));
    }

    #[test]
    fn every_action_has_its_own_id_uuid_and_a_known_symbol() {
        let mut ids: Vec<&str> = Action::ALL.iter().map(|a| a.id()).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), Action::ALL.len());
        for action in Action::ALL {
            assert_eq!(Action::from_id(action.id()), Some(action));
            assert_eq!(Action::from_uuid(&action.uuid()), Some(action));
            assert!(action.uuid().bytes().all(|b| b.is_ascii_lowercase() || b == b'.' || b == b'-'), "{}", action.uuid());
            let file = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../tools/symbol-vorlagen")
                .join(format!("{}.svg", action.icon()));
            assert!(file.is_file(), "Lucide symbol missing: {}", action.icon());
        }
    }

    #[test]
    fn every_action_has_a_name_and_a_tooltip_in_both_languages() {
        for lang in crate::language::Lang::ALL {
            for action in Action::ALL {
                for key in [action.label_key(), action.tooltip_key()] {
                    assert!(crate::language::text_in(lang, &key).is_some_and(|t| !t.is_empty()), "{key} {lang:?}");
                }
            }
        }
    }

    #[test]
    fn every_page_has_a_name_in_both_languages_and_stands_once() {
        let mut seen = PAGES.to_vec();
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(seen.len(), PAGES.len());
        for lang in crate::language::Lang::ALL {
            for page in PAGES {
                assert!(crate::language::text_in(lang, page).is_some(), "{page} {lang:?}");
            }
        }
        for page in [PAGE_CONTESTED, PAGE_FINANCES, PAGE_CONTROLS] {
            assert!(page_allowed(page), "{page}");
        }
    }

    #[test]
    fn the_commands_of_the_plugin_are_read_and_written_the_same_way() {
        let all = [
            Command::Overlay,
            Command::Main,
            Command::RefineryScan,
            Command::Blueprint,
            Command::Stock,
            Command::UnitStock,
            Command::Search,
            Command::Picture,
            Command::AntiAfk,
            Command::MiningScanner,
            Command::ClickThrough,
            Command::Sounds,
            Command::Page("hf_bergbau".into()),
            Command::GameAction { action: "v_toggle_mining_laser_type".into(), phase: Phase::Down },
            Command::GameAction { action: "v_toggle_mining_laser_type".into(), phase: Phase::Up },
            Command::GameActionsInUse(vec!["v_eject".into(), "pl_use".into()]),
            Command::DeviceHint(Some("Stream Deck Neo".into())),
            Command::DeviceHint(None),
            Command::BindingsFolder,
            Command::PluginVersion("5.7.0.12345".into()),
        ];
        for command in all {
            assert_eq!(parse_command(command_body(&command).as_bytes()), Ok(command.clone()), "{command:?}");
        }
        assert_eq!(parse_command(br#"{"id":"contested"}"#), Ok(Command::Page(PAGE_CONTESTED.into())));
        assert_eq!(parse_command(br#"{"id":"konto"}"#), Ok(Command::Page(PAGE_FINANCES.into())));
    }

    #[test]
    fn every_press_on_the_main_window_does_what_the_key_shows() {
        assert_eq!(MainWindow::of(false, false).press(), MainPress::Open, "closed: open");
        assert_eq!(MainWindow::of(false, true).press(), MainPress::Open, "not shown counts as closed");
        assert_eq!(MainWindow::of(true, false).press(), MainPress::ToFront, "behind: to the front, not closed");
        assert_eq!(MainWindow::of(true, true).press(), MainPress::Close, "in front: closed with one press");
        for window in [MainWindow::Closed, MainWindow::Behind, MainWindow::Front] {
            assert_eq!(MainWindow::from_id(window.id()), Some(window));
        }
    }

    #[test]
    fn plugin_versions_compare_number_by_number() {
        assert!(version_older("5.6.2.0", "5.6.2.12"));
        assert!(version_older("5.6.2.99999", "5.7.0.1"));
        assert!(version_older("5.7.0.9", "5.7.0.10"), "as numbers, not as letters");
        assert!(version_older("5.7", "5.7.0.1"), "a missing number counts as 0");
        assert!(!version_older("5.7.0.10", "5.7.0.9"));
        assert!(!version_older("5.7.0.10", "5.7.0.10"), "the same is not older");
        assert!(!version_older("unbekannt", "5.7.0.1"), "what is no version is never older");
    }

    #[test]
    fn the_version_of_a_plugin_file_is_read_from_its_manifest() {
        let mut zip = crate::zip::Builder::new((2026, 1, 1, 0, 0, 0));
        zip.add("com.xharig.versekit.sdPlugin/manifest.json", br#"{"Version":"5.7.0.123"}"#).expect("add");
        zip.add("com.xharig.versekit.sdPlugin/profiles/x/manifest.json", br#"{"Version":"9.9.9.9"}"#).expect("add");
        let file = zip.finish().expect("zip");
        assert_eq!(plugin_file_version(&file).as_deref(), Some("5.7.0.123"));
        assert_eq!(plugin_file_version(b"kein zip"), None);
    }

    #[test]
    fn a_command_that_is_not_one_is_refused_with_its_reason() {
        let cases: &[(&[u8], Refusal)] = &[
            (b"kein json", Refusal::Json),
            (b"[1,2]", Refusal::Json),
            (br#"{"ID":"overlay"}"#, Refusal::Id),
            (br#"{"id":"format_c"}"#, Refusal::Id),
            (br#"{"id":"seite"}"#, Refusal::Page),
            (br#"{"id":"seite","seite":"s_af_seite"}"#, Refusal::Page),
            (br#"{"id":"seite","seite":"../etwas"}"#, Refusal::Page),
            (br#"{"id":"spielaktion","phase":"runter"}"#, Refusal::GameAction),
            (br#"{"id":"spielaktion","aktion":"v eject","phase":"runter"}"#, Refusal::GameAction),
            (br#"{"id":"spielaktion","aktion":"v_eject"}"#, Refusal::Phase),
            (br#"{"id":"spielaktion","aktion":"v_eject","phase":"halten"}"#, Refusal::Phase),
            (br#"{"id":"spielaktionen_melden","aktionen":"v_eject"}"#, Refusal::List),
            (br#"{"id":"spielaktionen_melden","aktionen":[1]}"#, Refusal::List),
            (br#"{"id":"plugin_version"}"#, Refusal::Json),
            (br#"{"id":"plugin_version","version":"5.7.0.x"}"#, Refusal::Json),
            (br#"{"id":"plugin_version","version":"1.2.3.4.5"}"#, Refusal::Json),
        ];
        for (body, refusal) in cases {
            assert_eq!(parse_command(body), Err(*refusal), "{}", String::from_utf8_lossy(body));
        }
        let long: Vec<String> = (0..=MAX_REPORTED_ACTIONS).map(|n| format!("a{n}")).collect();
        let body = json!({"id": "spielaktionen_melden", "aktionen": long}).to_string();
        assert_eq!(parse_command(body.as_bytes()), Err(Refusal::List));
    }

    /// Names of the longest form a game action may have, each one different.
    fn longest_names(count: usize) -> Vec<String> {
        (0..count).map(|n| format!("v_{n:04}_{}", "x".repeat(96 - 7))).collect()
    }

    #[test]
    fn a_report_of_the_most_actions_with_the_longest_names_fits_into_one_body_and_one_more_is_refused() {
        let most = longest_names(MAX_REPORTED_ACTIONS);
        assert!(most.iter().all(|n| n.len() == 96 && plausible_action_name(n)));
        let body = command_body(&Command::GameActionsInUse(most.clone()));
        assert!(body.len() <= MAX_BODY, "{} bytes", body.len());
        assert_eq!(parse_command(body.as_bytes()), Ok(Command::GameActionsInUse(most)));
        let one_more = command_body(&Command::GameActionsInUse(longest_names(MAX_REPORTED_ACTIONS + 1)));
        assert_eq!(parse_command(one_more.as_bytes()), Err(Refusal::List));
        // The profiles of the + XL name 162 game actions; a report of more than that goes through whole.
        let xl = longest_names(200);
        let body = command_body(&Command::GameActionsInUse(xl.clone()));
        assert!(body.len() <= MAX_BODY, "{} bytes", body.len());
        assert_eq!(parse_command(body.as_bytes()), Ok(Command::GameActionsInUse(xl)));
    }

    #[test]
    fn a_press_and_its_release_count_as_one_press() {
        let down = Command::GameAction { action: "v_eject".into(), phase: Phase::Down };
        let up = Command::GameAction { action: "v_eject".into(), phase: Phase::Up };
        assert_eq!(down.limit_key(), "spielaktion:v_eject");
        assert_eq!(up.limit_key(), "", "the release is not counted");
    }

    /// Lines of a real game log (4.x): leaving the seat of the pilot after landing, choosing a quantum target in the
    /// same ship, and another line of the same ship.
    const RELEASED: &str = "<2026-11-25T20:26:33.338Z> [Notice] <Vehicle Control Flow> CVehicleMovementBase::ClearDriver: \
        Local client node [200000000001] releasing control token for 'RSI_Constellation_Andromeda_GS_465232524500' \
        [465232524500] [Team_CGP4][Vehicle]";
    const TARGET: &str = "<2026-11-25T20:05:53.305Z> [Notice] <Player Selected Quantum Target - Local> [ItemNavigation][CL][36044] \
        | NOT AUTH | RSI_Constellation_Andromeda_GS_465232524500[465232524500]|CSCItemNavigation::OnPlayerSelectedQuantumTarget\
        |Player has selected point rs_ext_nyx-castra_jp1 as their destination, routing locally [Team_CGP4][QuantumTravel]";
    const NOT_GRANTED: &str = "<2026-11-25T20:12:45.100Z> [Notice] <SCItemVehicleLandingGearSystem::OnEngineStateChange()> \
        Failed to detach vehicle due to control token not being granted [Team_CGP4][Docker]";

    #[test]
    fn leaving_the_seat_asks_for_the_page_on_foot_and_other_lines_ask_for_nothing() {
        assert_eq!(device_page_in(RELEASED), Some((DevicePage::OnFoot, crate::logtime::seconds("2026-11-25T20:26:33.338Z"))));
        assert!(device_page_in(RELEASED).is_some_and(|(_, at)| at.is_some()), "the stamp of the line");
        for other in [TARGET, NOT_GRANTED, "", "<Vehicle Control Flow> something else"] {
            assert_eq!(device_page_in(other), None, "{other}");
        }
        let section = format!("{TARGET}\n{RELEASED}\n{NOT_GRANTED}\n");
        assert_eq!(device_page_in(&section).map(|(page, _)| page), Some(DevicePage::OnFoot), "found among other lines");
    }

    #[test]
    fn the_page_on_foot_is_found_by_its_name_in_both_languages() {
        let names = |list: &[&str]| list.iter().map(|s| (*s).to_string()).collect::<Vec<_>>();
        let plus_xl = names(&["", "Bergbau", "Bergung", "Dogfight", "Flug", "Zu Fuß", "Fahrzeug"]);
        assert_eq!(DevicePage::OnFoot.number_in(&plus_xl), Some(5));
        let mk2 = names(&["Pages", "Start 1", "Flight 3", "On Foot 1", "On Foot 2", "Vehicle 1"]);
        assert_eq!(DevicePage::OnFoot.number_in(&mk2), Some(3), "the first of several");
        assert_eq!(DevicePage::OnFoot.number_in(&names(&["Start", "Flug"])), None);
    }

    #[test]
    fn the_wish_for_a_page_goes_into_the_state_and_comes_back() {
        let value = device_page_value(Some((3, DevicePage::OnFoot)));
        let state = State::parse(&json!({"version": PROTOCOL, "rev": 1, FIELD_DEVICE_PAGE: value}).to_string()).expect("state");
        assert_eq!(device_page_of(&state), Some((3, DevicePage::OnFoot)));
        let none = State::parse(&json!({"version": PROTOCOL, "rev": 1, FIELD_DEVICE_PAGE: device_page_value(None)}).to_string());
        assert_eq!(device_page_of(&none.expect("state")), None);
        let unknown = State::parse(&json!({"version": PROTOCOL, "rev": 1, FIELD_DEVICE_PAGE: {"nr": 1, "seite": "mond"}}).to_string());
        assert_eq!(device_page_of(&unknown.expect("state")), None, "a page this side does not know");
        for page in DevicePage::ALL {
            assert_eq!(DevicePage::from_id(page.id()), Some(page));
        }
    }

    #[test]
    fn the_running_contracts_go_into_the_state_and_come_back_with_the_objective_now() {
        let line = |title: &str, goals: &[&str], steps: Option<(usize, usize)>| crate::live_contracts::Line {
            title: title.into(),
            text: crate::overlay::Message::Plain(String::new()),
            goals: goals.iter().map(|g| g.to_string()).collect(),
            steps,
            grey: false,
            count: 1,
            complete: false,
        };
        let value = contracts_value(&[
            line("Fracht", &["Fracht abholen bei Everus Harbour", "Abliefern"], Some((1, 3))),
            line("Jagd", &[], None),
        ]);
        let state = State::parse(&json!({"version": PROTOCOL, "rev": 1, FIELD_CONTRACTS: value}).to_string()).expect("state");
        let cards = contracts_of(&state);
        assert_eq!(cards.len(), 2);
        assert_eq!(cards[0].objective.as_deref(), Some("Fracht abholen bei Everus Harbour"), "the first open objective");
        assert_eq!(cards[0].step_now(), Some((2, 3)), "one done: the second of three");
        assert_eq!((cards[1].objective.as_deref(), cards[1].step_now()), (None, None), "nothing known, nothing made up");
        assert_eq!(ContractCard { title: "x".into(), objective: None, steps: Some((3, 3)) }.step_now(), Some((3, 3)), "all done");
        let none = State::parse(&json!({"version": PROTOCOL, "rev": 1}).to_string()).expect("state");
        assert!(contracts_of(&none).is_empty());
    }

    #[test]
    fn the_readiness_goes_into_the_state_and_comes_back_and_a_clock_reads_minutes_and_seconds() {
        let ready = Readiness { game_runs: true, log_found: Some(true), keys_missing: 3 };
        let state = State::parse(&json!({"version": PROTOCOL, "rev": 1, FIELD_STATUS: ready.value()}).to_string()).expect("state");
        assert_eq!(Readiness::of(&state), Some(ready));
        let unsure = Readiness { log_found: None, ..ready };
        let state = State::parse(&json!({"version": PROTOCOL, "rev": 1, FIELD_STATUS: unsure.value()}).to_string()).expect("state");
        assert_eq!(Readiness::of(&state).and_then(|r| r.log_found), None, "nobody looked yet");
        assert_eq!(clock(725), "12:05");
        assert_eq!(clock(59), "00:59");
        assert_eq!(clock(3725), "1:02:05");
    }

    #[test]
    fn a_game_action_without_a_text_of_its_own_takes_the_short_name_of_the_profiles() {
        use crate::language::Lang;
        assert_eq!(short_name("v_toggle_guns_mode", Lang::De), Some("WAFFEN-MODUS"));
        assert_eq!(short_name("spaceship_weapons.v_toggle_guns_mode", Lang::En), Some("GUN MODE"));
        assert_eq!(short_name("v_nothing_known", Lang::De), None);
        // No German text: the short name before the English one; a German text of the game stays.
        assert_eq!(game_action_name("v_toggle_jump_request", "Toggle Jump Request", false, Lang::De), "SPRUNG");
        assert_eq!(game_action_name("v_ifcs_vector_decoupling_toggle", "Ifcs Vector Decoupling Toggle", false, Lang::De), "ENTKOPPELT");
        assert_eq!(game_action_name("v_toggle_jump_request", "Sprung anfordern", true, Lang::De), "Sprung anfordern");
        assert_eq!(game_action_name("v_nothing_known", "Toggle Nothing", false, Lang::De), "Toggle Nothing");
        // A line break of the game is a space.
        assert_eq!(game_action_name("v_x", "Andocken\\n(Einleiten)", true, Lang::De), "Andocken (Einleiten)");
    }

    #[test]
    fn eject_emergency_exit_self_destruct_and_all_power_are_dangerous_by_themselves() {
        for action in ["v_eject", "v_emergency_exit", "v_self_destruct", "v_power_toggle", "seat_general.v_eject"] {
            assert!(dangerous_by_itself(action), "{action}");
        }
        for action in ["v_power_toggle_shields", "v_eject_cinematic", "", "v_lights"] {
            assert!(!dangerous_by_itself(action), "{action}");
        }
    }

    #[test]
    fn the_health_goes_into_the_state_as_current_and_maximum_and_comes_back_in_per_cent() {
        let state = |value: Value| State::parse(&json!({"version": PROTOCOL, "rev": 1, FIELD_HEALTH: value}).to_string()).expect("state");
        assert_eq!(health_of(&state(health_value(Some((87.0, 100.0))))), Some(87));
        assert_eq!(health_of(&state(health_value(Some((45.0, 150.0))))), Some(30));
        assert_eq!(health_of(&state(health_value(Some((120.0, 100.0))))), Some(100), "never above all");
        assert_eq!(health_of(&state(health_value(None))), None, "no telemetry: none");
        assert_eq!(health_of(&state(health_value(Some((5.0, 0.0))))), None, "no maximum: none");
        assert_eq!(Action::from_uuid("com.xharig.versekit.gesundheit"), Some(Action::Health));
    }

    #[test]
    fn the_ship_goes_into_the_state_and_comes_back_and_none_stays_none() {
        let state = State::parse(&json!({"version": PROTOCOL, "rev": 1, FIELD_SHIP: ship_value(Some("Aegis Sabre"))}).to_string()).expect("state");
        assert_eq!(ship_of(&state).as_deref(), Some("Aegis Sabre"));
        let none = State::parse(&json!({"version": PROTOCOL, "rev": 1, FIELD_SHIP: ship_value(None)}).to_string()).expect("state");
        assert_eq!(ship_of(&none), None);
        assert!(page_allowed(PAGE_SHIPS));
    }

    #[test]
    fn a_state_of_another_version_is_not_read_and_unknown_fields_are_kept() {
        let state = State::parse(r#"{"version":1,"rev":4,"sprache":"en","neu":{"x":1}}"#).expect("state");
        assert_eq!(state.rev, 4);
        assert_eq!(state.language(), crate::language::Lang::En);
        assert!(state.get("neu").is_some());
        assert_eq!(State::parse(r#"{"version":2,"rev":4}"#), None);
        assert_eq!(State::parse(r#"{"rev":4}"#), None);
    }
}
