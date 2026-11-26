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

//! What the plugin does with the events of the Stream Deck program and the state of VerseKit. Every function
//! returns the messages for the Stream Deck program instead of sending them, so the whole plugin can be checked
//! without a device.

use std::collections::{BTreeMap, HashMap};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use versekit_core::language::{self, Lang};
use versekit_core::streamdeck::{self, Action, Command, Phase, State};

use crate::render::{Badge, Look, Tone};
use crate::verse::{Missing, Verse};

/// Where a key sits: a key, or a dial of a Stream Deck + XL.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Controller {
    Keypad,
    Encoder,
}

/// One key or dial on a device.
#[derive(Clone, Debug, PartialEq)]
pub struct Key {
    pub action: Action,
    pub settings: Value,
    pub device: String,
    pub controller: Controller,
}

/// How a game action can be pressed, as VerseKit says.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Binding {
    pub kind: String,
    pub key: String,
    /// The key is held with Alt Gr, which the game does not see as it is bound.
    pub unsafe_key: bool,
    pub name: String,
    pub group: String,
    /// The `actionmap` of the game the action belongs to.
    pub map: String,
}

/// A device and the profiles shipped for it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Device {
    pub id: String,
    pub kind: u64,
    pub name: String,
}

/// The kinds of device with a profile of their own: the kind (as `deviceInfo.type` and the manifest say), the name
/// of the shipped file without the language, and the model the Stream Deck program writes into such a profile
/// (`None` while no profile of that kind was exported). The model of the + XL comes from the standard profile of
/// the Stream Deck program, not yet from a device.
pub const PROFILE_DEVICES: [(u64, &str, Option<&str>); 6] = [
    (0, "VerseKit-StreamDeck-MK2", Some("20GBA9901")),
    (1, "VerseKit-StreamDeck-Mini", Some("20GAI9901")),
    (2, "VerseKit-StreamDeck-XL", Some("20GAT9901")),
    (3, "VerseKit-StreamDeck-Mobile", Some("VSD2/WiFi")),
    (9, "VerseKit-StreamDeck-Neo", Some("20GBJ9901")),
    (13, "VerseKit-StreamDeck-PlusXL", Some("20GBX9901")),
];

/// The symbol of a page key: that of the page where the profiles have one, otherwise the one of the action.
pub fn page_icon(page: &str) -> &'static str {
    match page {
        "hf_hangar" => "plane",
        "hf_meinlager" => "package",
        "hf_staffellager" => "boxes",
        "hf_bergbau" => "compass",
        "hf_bergung" => "wrench",
        "hf_liste" => "clipboard-list",
        "hf_herstellung" => "zap",
        "hf_verkauf" => "coins",
        "hf_routen" => "route",
        "hf_finanzen" => "hand-coins",
        "hf_st_uebersicht" => "chart-no-axes-combined",
        "hf_allgemein" => "settings",
        "hf_contested" => "timer",
        _ => "panel-left",
    }
}

/// The name of the profile shipped for a kind of device, without the language; `None` for a kind without one.
pub fn profile_base(kind: u64) -> Option<&'static str> {
    PROFILE_DEVICES.iter().find(|(k, _, _)| *k == kind).map(|(_, base, _)| *base)
}

/// The name a shipped profile has in the manifest and in `switchToProfile`: its path without the extension.
pub fn profile_name(file: &str) -> String {
    format!("profiles/{file}")
}

/// An emote key let go before this is a tap and goes back to its page; held longer it stays.
pub const EMOTE_TAP: Duration = Duration::from_millis(500);
/// The `actionmap` of the emotes of the game.
pub const EMOTE_MAP: &str = "player_emotes";
/// A counted switch held this long is set back to off instead of switched.
pub const SWITCH_RESET: Duration = Duration::from_secs(2);

/// A switch of the game that does not say its state: the plugin counts the presses.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Counted {
    pub on: bool,
    /// Set back by the player: the state is known. Every counted press makes it unsure again.
    pub sure: bool,
}

/// The plugin.
pub struct Plugin {
    /// The UUID the Stream Deck program gave this run of the plugin.
    pub uuid: String,
    pub keys: BTreeMap<String, Key>,
    pub state: Option<State>,
    /// Why VerseKit gave no state; `None` while it answers.
    pub missing: Option<Missing>,
    pub bindings: HashMap<String, Binding>,
    bindings_mark: Option<String>,
    /// The pictures last sent, so the same one is not sent again.
    shown: HashMap<String, String>,
    /// Game actions held down by a key, by its context.
    held: HashMap<String, String>,
    /// The game actions reported to VerseKit last.
    reported: Vec<String>,
    /// The profiles that lie in the folder of the plugin (`VerseKit-Mobile-DE`, …).
    pub profiles: Vec<String>,
    /// Devices whose profile was offered already (kept in the global settings).
    offered: Vec<String>,
    global_read: bool,
    devices: Vec<Device>,
    /// The hint about a device without a profile last told VerseKit.
    told_hint: Option<Option<String>>,
    /// When a key that tells a tap from a hold went down, by its context.
    pressed: HashMap<String, Instant>,
    /// The counted switches, by context; lost when the plugin starts anew, then they are unsure.
    pub counted: HashMap<String, Counted>,
    /// The time the plugin goes by; a test sets it, otherwise the clock.
    pub fixed_now: Option<Instant>,
    /// The game actions on the keys of each shipped profile, all pages (`VerseKit-…-DE` → names).
    pub profile_actions: HashMap<String, Vec<String>>,
    /// The game actions of every key shown in this run.
    seen_actions: Vec<String>,
    /// The short series of pictures a dial runs through after a turn or a press: the pictures, the next one, when.
    motion: HashMap<String, (Vec<crate::render::Motion>, usize, Instant)>,
    /// When each dial was last sent a picture, for the limit per second.
    dial_sent: HashMap<String, std::collections::VecDeque<Instant>>,
    /// Dials whose new picture waits for the limit.
    dial_waiting: std::collections::HashSet<String>,
    /// The running contract each objective dial stands at, by its place in the list.
    contract_at: HashMap<String, usize>,
    /// Dangerous game actions held down, by context: the action, since when, sent already, the last picture.
    guarded: HashMap<String, Guarded>,
    /// Keys let go too early, by context: until when they say „HALTEN“ in the colour of warnings.
    hold_hint: HashMap<String, Instant>,
    /// The names of the pages of each shipped profile (`VerseKit-…-DE` → names in their order).
    pub pages: HashMap<String, Vec<String>>,
    /// The number of the last wish of VerseKit for a page of the device, switched or passed over.
    page_wish: Option<u64>,
}

/// The reasons of [`Plugin::game_block`] that are a missing binding on the keyboard: a press on such a key opens the
/// page where the missing keys are put in.
pub const MISSING_BINDING: [&str; 5] =
    ["sd_ga_nicht_belegt", "sd_ga_nur_maus", "sd_ga_nur_joystick", "sd_ga_nur_gamepad", "sd_ga_nicht_sendbar"];

/// How long a dangerous game action (`gefaehrlich: true` in the profile) must be held before it is sent.
pub const HOLD: Duration = Duration::from_secs(1);
/// How long a key let go too early says „HALTEN“ in the colour of warnings.
pub const HOLD_HINT: Duration = Duration::from_secs(1);

/// A dangerous game action held down.
#[derive(Clone, Debug)]
struct Guarded {
    action: String,
    since: Instant,
    sent: bool,
    drawn: Instant,
}

/// The page of the main window a press on the objective dial opens.
pub const PAGE_CONTRACT_LOG: &str = "hf_auftragslog";

/// The time between two pictures of a dial's series.
pub const DIAL_FRAME: Duration = Duration::from_millis(100);
/// The most pictures a dial gets per second (the rule of Elgato for the touch strip).
pub const DIAL_RATE: usize = 10;

/// The settings of a key that name a game action: a key, or the two sides and the press of a dial.
pub const ACTION_FIELDS: [&str; 4] = ["aktion", "aktion_links", "aktion_rechts", "aktion_druck"];

/// The game actions on the keys and dials of every page of a profile file (game action and emote keys), each once,
/// in the order they are found; empty for a file that is none.
pub fn profile_game_actions(file: &[u8]) -> Vec<String> {
    let Ok(entries) = versekit_core::zip::entries(file) else { return Vec::new() };
    let wanted = [Action::GameAction.uuid(), Action::Emote.uuid()];
    let mut names: Vec<String> = Vec::new();
    for entry in &entries {
        let lower = entry.name.to_ascii_lowercase();
        if !lower.contains("/profiles/") || !lower.ends_with("/manifest.json") {
            continue;
        }
        let Some(page) = versekit_core::zip::read(file, entry).ok().and_then(|b| serde_json::from_slice::<Value>(&b).ok()) else {
            continue;
        };
        for controller in page["Controllers"].as_array().into_iter().flatten() {
            for action in controller["Actions"].as_object().into_iter().flat_map(|m| m.values()) {
                if !action["UUID"].as_str().is_some_and(|u| wanted.iter().any(|w| w == u)) {
                    continue;
                }
                for field in ACTION_FIELDS {
                    if let Some(name) = action["Settings"][field].as_str().filter(|n| !n.is_empty())
                        && !names.iter().any(|n| n == name)
                    {
                        names.push(name.to_string());
                    }
                }
            }
        }
    }
    names
}

/// The names of the pages of a profile file, in the order of the profile (the number `switchToProfile` takes); empty
/// for a file that is none.
pub fn profile_pages(file: &[u8]) -> Vec<String> {
    let Ok(entries) = versekit_core::zip::entries(file) else { return Vec::new() };
    let json_of = |name: &str| -> Option<Value> {
        let entry = entries.iter().find(|e| e.name.eq_ignore_ascii_case(name))?;
        serde_json::from_slice(&versekit_core::zip::read(file, entry).ok()?).ok()
    };
    let Some(top) = entries.iter().find(|e| {
        let mut parts = e.name.split('/');
        matches!((parts.next(), parts.next(), parts.next()), (Some(folder), Some("manifest.json"), None) if folder.ends_with(".sdProfile"))
    }) else {
        return Vec::new();
    };
    let folder = top.name.trim_end_matches("manifest.json");
    let Some(manifest) = json_of(&top.name) else { return Vec::new() };
    manifest["Pages"]["Pages"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(|page| {
            json_of(&format!("{folder}Profiles/{page}/manifest.json"))
                .and_then(|m| m["Name"].as_str().map(String::from))
                .unwrap_or_default()
        })
        .collect()
}

/// A line for the log of the plugin (`com.xharig.versekit0.log` of the Stream Deck program). Never with the key of
/// the interface or a path of the player.
pub fn note(text: &str) -> Value {
    json!({"event": "logMessage", "payload": {"message": text}})
}

fn event(name: &str, context: &str, payload: Value) -> Value {
    if payload.is_null() {
        json!({"event": name, "context": context})
    } else {
        json!({"event": name, "context": context, "payload": payload})
    }
}

impl Plugin {
    pub fn new(uuid: &str, profiles: Vec<String>) -> Plugin {
        Plugin {
            uuid: uuid.to_string(),
            keys: BTreeMap::new(),
            state: None,
            missing: None,
            bindings: HashMap::new(),
            bindings_mark: None,
            shown: HashMap::new(),
            held: HashMap::new(),
            reported: Vec::new(),
            profiles,
            offered: Vec::new(),
            global_read: false,
            devices: Vec::new(),
            told_hint: None,
            pressed: HashMap::new(),
            counted: HashMap::new(),
            fixed_now: None,
            profile_actions: HashMap::new(),
            seen_actions: Vec::new(),
            motion: HashMap::new(),
            dial_sent: HashMap::new(),
            dial_waiting: std::collections::HashSet::new(),
            contract_at: HashMap::new(),
            guarded: HashMap::new(),
            hold_hint: HashMap::new(),
            pages: HashMap::new(),
            page_wish: None,
        }
    }

    fn now(&self) -> Instant {
        self.fixed_now.unwrap_or_else(Instant::now)
    }

    /// A game action key that counts its switch (only a key, not a dial).
    fn counts(key: &Key) -> bool {
        key.action == Action::GameAction && key.controller == Controller::Keypad && key.settings["umschalter"] == true
    }

    /// The first messages after the registration.
    pub fn start(&self) -> Vec<Value> {
        vec![json!({"event": "getGlobalSettings", "context": self.uuid})]
    }

    fn lang(&self) -> Lang {
        self.state.as_ref().map_or(Lang::De, State::language)
    }

    fn t(&self, key: &str) -> String {
        language::t_in(self.lang(), key)
    }

    /// Handles one event of the Stream Deck program.
    pub fn on_event(&mut self, message: &Value, verse: &mut dyn Verse) -> Vec<Value> {
        let name = message["event"].as_str().unwrap_or("");
        let context = message["context"].as_str().unwrap_or("").to_string();
        let payload = &message["payload"];
        match name {
            "willAppear" => {
                let uuid = message["action"].as_str().unwrap_or("");
                let Some(action) = Action::from_uuid(uuid) else {
                    return vec![note(&format!("willAppear einer unbekannten Aktion: {uuid}"))];
                };
                let controller =
                    if payload["controller"].as_str() == Some("Encoder") { Controller::Encoder } else { Controller::Keypad };
                let key = Key {
                    action,
                    settings: payload["settings"].clone(),
                    device: message["device"].as_str().unwrap_or("").to_string(),
                    controller,
                };
                let line = format!("willAppear {} {context} Gerät {} {:?}", action.id(), key.device, controller);
                self.keys.insert(context.clone(), key);
                self.shown.remove(&context);
                let mut out = vec![note(&line), event("setTitle", &context, json!({"title": ""}))];
                out.extend(self.draw(&context));
                out.extend(self.report_game_actions(verse));
                out
            }
            "willDisappear" => {
                let mut out = self.release(&context, verse);
                self.keys.remove(&context);
                self.shown.remove(&context);
                self.motion.remove(&context);
                self.dial_waiting.remove(&context);
                self.guarded.remove(&context);
                self.hold_hint.remove(&context);
                out.extend(self.report_game_actions(verse));
                out
            }
            "didReceiveSettings" => {
                if let Some(key) = self.keys.get_mut(&context) {
                    key.settings = payload["settings"].clone();
                }
                self.shown.remove(&context);
                let mut out = self.draw(&context);
                out.extend(self.report_game_actions(verse));
                out
            }
            "keyDown" => self.press(&context, verse),
            "keyUp" => self.release(&context, verse),
            "dialDown" => self.dial_press(&context, Phase::Down, verse),
            "dialUp" => self.dial_press(&context, Phase::Up, verse),
            "dialRotate" => {
                let ticks = payload["ticks"].as_i64().unwrap_or(0);
                self.dial_turn(&context, ticks, verse)
            }
            "propertyInspectorDidAppear" => self.inspector(&context),
            "sendToPlugin" => self.inspector_asked(&context, payload, verse),
            "deviceDidConnect" => {
                let device = Device {
                    id: message["device"].as_str().unwrap_or("").to_string(),
                    kind: message["deviceInfo"]["type"].as_u64().unwrap_or(u64::MAX),
                    name: message["deviceInfo"]["name"].as_str().unwrap_or("").to_string(),
                };
                let line = format!("Gerät {} Typ {} „{}“", device.id, device.kind, device.name);
                self.devices.retain(|d| d.id != device.id);
                self.devices.push(device);
                let mut out = vec![note(&line)];
                out.extend(self.offer_profiles(verse));
                out.extend(self.report_game_actions(verse));
                out
            }
            "deviceDidDisconnect" => {
                let id = message["device"].as_str().unwrap_or("");
                self.devices.retain(|d| d.id != id);
                Vec::new()
            }
            "didReceiveGlobalSettings" => {
                self.offered = payload["settings"]["profil_angeboten"]
                    .as_array()
                    .map(|list| list.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                    .unwrap_or_default();
                self.global_read = true;
                self.offer_profiles(verse)
            }
            "systemDidWakeUp" => {
                self.shown.clear();
                self.redraw_all()
            }
            _ => Vec::new(),
        }
    }

    /// Asks VerseKit for the state; redraws what changed.
    pub fn poll(&mut self, verse: &mut dyn Verse) -> Vec<Value> {
        let since = if self.missing.is_none() { self.state.as_ref().map(|s| s.rev) } else { None };
        let before_lang = self.lang();
        let was_away = self.missing.is_some() || self.state.is_none();
        match verse.state(since) {
            Ok(None) => return Vec::new(),
            Ok(Some(state)) => {
                self.state = Some(state);
                self.missing = None;
                if was_away {
                    // VerseKit may have started anew and knows nothing of the keys, nor of this plugin.
                    self.reported.clear();
                    self.told_hint = None;
                    self.bindings_mark = None;
                    let _ = verse.command(&Command::PluginVersion(crate::pack::version()));
                }
            }
            Err(missing) => {
                if self.missing.as_ref() == Some(&missing) {
                    return Vec::new();
                }
                let text = format!("VerseKit antwortet nicht: {missing:?}");
                self.missing = Some(missing);
                let mut out = vec![note(&text)];
                out.extend(self.redraw_all());
                return out;
            }
        }
        let mut out = Vec::new();
        if was_away {
            out.push(note("VerseKit antwortet"));
        }
        out.extend(self.follow_page_wish(was_away));
        let mark = self.state.as_ref().and_then(|s| s.get("belegung_stand")).and_then(Value::as_str).map(String::from);
        if self.missing.is_none() && mark != self.bindings_mark {
            self.load_bindings(verse);
            self.bindings_mark = mark;
        }
        if before_lang != self.lang() {
            self.shown.clear();
        }
        out.extend(self.redraw_all());
        if self.missing.is_none() {
            out.extend(self.report_game_actions(verse));
            out.extend(self.offer_profiles(verse));
        }
        out
    }

    fn load_bindings(&mut self, verse: &mut dyn Verse) {
        let Ok(value) = verse.bindings() else { return };
        self.bindings = value["aktionen"]
            .as_array()
            .map(|list| {
                list.iter()
                    .filter_map(|item| {
                        let id = item["id"].as_str()?.to_string();
                        let binding = Binding {
                            kind: item["art"].as_str().unwrap_or("unbekannt").to_string(),
                            key: item["taste"].as_str().unwrap_or("").to_string(),
                            unsafe_key: item["unsicher"].as_bool().unwrap_or(false),
                            name: item["name"].as_str().unwrap_or(&id).to_string(),
                            group: item["gruppe"].as_str().unwrap_or("").to_string(),
                            map: item["karte"].as_str().unwrap_or("").to_string(),
                        };
                        Some((id, binding))
                    })
                    .collect()
            })
            .unwrap_or_default();
    }

    fn redraw_all(&mut self) -> Vec<Value> {
        let contexts: Vec<String> = self.keys.keys().cloned().collect();
        contexts.iter().flat_map(|c| self.draw(c)).collect()
    }

    /// What the field of a dial shows now: its look as a key, in the form the setting of VerseKit asks for, in the
    /// picture of its series it stands at.
    fn dial_look(&self, context: &str, look: &Look) -> crate::render::DialLook {
        let warning = look.badge.as_ref().filter(|b| matches!(b.tone, Tone::Warning | Tone::Error)).map(|b| b.word.clone());
        let compact = self.state.as_ref().and_then(|s| s.get(streamdeck::FIELD_DIALS)).is_some_and(|d| d["kompakt"] == true);
        let motion = self.motion.get(context).and_then(|(frames, at, _)| frames.get(*at).copied()).unwrap_or_default();
        crate::render::DialLook {
            label: look.label.clone(),
            icon: look.icon,
            colour: look.colour.unwrap_or("gruen"),
            value: None,
            on: None,
            warning,
            compact,
            energy: self.keys.get(context).is_some_and(|k| k.settings["energie"] == true),
            motion,
            hold: look.hold,
        }
    }

    /// What an info dial shows, from the state VerseKit gives: the contested zones (the time to the next change of
    /// the hangar as a clock, and whether it is open) or whether all is ready (the game, the log, the keys). A dash
    /// where VerseKit gives nothing.
    fn info_look(&self, action: Action) -> crate::render::InfoLook {
        let none = || "—".to_string();
        let state = self.state.as_ref().filter(|_| self.missing.is_none());
        match action {
            Action::Contested => {
                let cycle = state.and_then(|s| s.get("contested"));
                let open = cycle.and_then(|c| c["offen"].as_bool());
                let value = cycle.and_then(|c| c["sekunden"].as_u64()).map_or_else(none, streamdeck::clock);
                let (line, tone) = match open {
                    Some(true) => (self.t("sd_z_offen"), Tone::Accent),
                    Some(false) => (self.t("sd_z_zu"), Tone::Muted),
                    None => (self.t("sd_z_keine"), Tone::Muted),
                };
                crate::render::InfoLook { label: self.t("sd_info_contested"), colour: "seite", value, line, tone }
            }
            // The game tells the money only with its telemetry: nothing is made up before.
            Action::Wallet => crate::render::InfoLook {
                label: self.t("sd_info_konto"),
                colour: "gruen",
                value: none(),
                line: self.t("sd_info_ab_telemetrie"),
                tone: Tone::Muted,
            },
            Action::Ship => {
                let (value, line, tone) = match state.and_then(streamdeck::ship_of) {
                    Some(name) => (name, self.t("sd_info_zuletzt"), Tone::Accent),
                    None => (none(), self.t("sd_info_kein_schiff"), Tone::Muted),
                };
                crate::render::InfoLook { label: self.t("sd_info_schiff"), colour: "gruen", value, line, tone }
            }
            // The health comes only with the telemetry of the game: before it, nothing is made up.
            Action::Health => {
                let (value, line, tone) = match state.and_then(streamdeck::health_of) {
                    Some(percent) => {
                        let (word, tone) = health_level(percent);
                        (format!("{percent} %"), self.t(word), tone)
                    }
                    None => (none(), self.t("sd_info_ab_telemetrie"), Tone::Muted),
                };
                crate::render::InfoLook { label: self.t("sd_info_gesundheit"), colour: "gruen", value, line, tone }
            }
            _ => {
                let label = self.t("sd_info_status");
                let Some(ready) = state.and_then(streamdeck::Readiness::of) else {
                    return crate::render::InfoLook { label, colour: "gruen", value: none(), line: self.t("sd_z_getrennt"), tone: Tone::Warning };
                };
                let value = self.t(if ready.game_runs { "sd_info_spiel" } else { "sd_info_kein_spiel" });
                let mut problems = Vec::new();
                if ready.log_found == Some(false) {
                    problems.push(self.t("sd_info_log_fehlt"));
                }
                match ready.keys_missing {
                    0 => {}
                    1 => problems.push(self.t("sd_info_tasten_1")),
                    n => problems.push(language::tf_in(self.lang(), "sd_info_tasten", &[("0", n.to_string().into())])),
                }
                let (line, tone) = if problems.is_empty() { (self.t("sd_info_bereit"), Tone::Accent) } else { (problems.join(" · "), Tone::Warning) };
                crate::render::InfoLook { label, colour: "gruen", value, line, tone }
            }
        }
    }

    /// What the objective dial shows: the running contract it stands at (the last one where the list got shorter).
    fn objective_look(&self, context: &str) -> crate::render::ObjectiveLook {
        let head = self.t("sd_auftrag_kopf");
        let cards = self.state.as_ref().map(streamdeck::contracts_of).unwrap_or_default();
        let Some(last) = cards.len().checked_sub(1) else {
            return crate::render::ObjectiveLook { head, text: self.t("sd_auftrag_keiner"), stand: None };
        };
        let card = &cards[self.contract_at.get(context).copied().unwrap_or(0).min(last)];
        let stand = card.step_now().map(|(now, all)| {
            language::tf_in(self.lang(), "sd_auftrag_stand", &[("0", now.to_string().into()), ("1", all.to_string().into())])
        });
        crate::render::ObjectiveLook { head, text: card.objective.clone().unwrap_or_else(|| card.title.clone()), stand }
    }

    /// May a dial get another picture now? At most [`DIAL_RATE`] within one second.
    fn dial_may_send(&mut self, context: &str, now: Instant) -> bool {
        let sent = self.dial_sent.entry(context.to_string()).or_default();
        while sent.front().is_some_and(|at| now.duration_since(*at) >= Duration::from_secs(1)) {
            sent.pop_front();
        }
        if sent.len() >= DIAL_RATE {
            return false;
        }
        sent.push_back(now);
        true
    }

    /// The picture of one key, sent only when it changed.
    fn draw(&mut self, context: &str) -> Vec<Value> {
        let Some(mut look) = self.look_of(context) else { return Vec::new() };
        // A dangerous game action: the ring of its hold, and „HALTEN“ in the colour of warnings when let go too early.
        look.hold = self.hold_progress(context);
        if self.hold_hint.contains_key(context) {
            look.badge = self.badge("sd_z_halten", true, Tone::Warning);
        }
        let Some(key) = self.keys.get(context) else { return Vec::new() };
        if key.controller == Controller::Encoder {
            let picture = match key.action {
                Action::Objective => crate::paint::objective_png(&self.objective_look(context)),
                Action::Contested | Action::Status | Action::Wallet | Action::Ship | Action::Health => {
                    crate::paint::info_png(&self.info_look(key.action))
                }
                _ => crate::paint::dial_png(&self.dial_look(context, &look)),
            };
            let url = crate::paint::data_url(&picture);
            if self.shown.get(context) == Some(&url) {
                self.dial_waiting.remove(context);
                return Vec::new();
            }
            let now = self.now();
            if !self.dial_may_send(context, now) {
                self.dial_waiting.insert(context.to_string());
                return Vec::new();
            }
            self.dial_waiting.remove(context);
            self.shown.insert(context.to_string(), url.clone());
            return vec![event("setFeedback", context, json!({crate::pack::DIAL_PICTURE: url}))];
        }
        let url = crate::paint::data_url(&crate::paint::png(&look, key.action));
        if self.shown.get(context) == Some(&url) {
            return Vec::new();
        }
        let first = self.shown.insert(context.to_string(), url.clone()).is_none();
        let mut out = vec![event("setImage", context, json!({"image": url, "target": 0}))];
        if first {
            out.push(note(&format!("setImage {} {context}: {} Zeichen, Symbol {}", key.action.id(), url.len(), look.icon)));
        }
        out
    }

    fn badge(&self, key: &str, filled: bool, tone: Tone) -> Option<Badge> {
        Some(Badge { word: self.t(key), filled, tone })
    }

    /// The name on a key: the short one (`sd_k_<id>`) where a key shows a state below it, so the name keeps its
    /// letters of 18 pixels; otherwise the name of the action.
    fn key_label(&self, action: Action) -> String {
        language::text_in(self.lang(), &format!("sd_k_{}", action.id()))
            .map(String::from)
            .unwrap_or_else(|| self.t(&action.label_key()))
    }

    /// What a key shows now.
    pub fn look(&self, key: &Key) -> Look {
        let action = key.action;
        let mut look =
            Look { icon: action.icon(), icon_tone: Tone::Accent, label: self.key_label(action), badge: None, colour: None, hold: None };
        if matches!(action, Action::GameAction | Action::Emote) {
            self.game_style(key, &mut look);
            Self::profile_look(key, &mut look);
        }
        if self.missing.is_some() || self.state.is_none() {
            look.icon = "unplug";
            look.icon_tone = Tone::Warning;
            look.badge = self.badge("sd_z_getrennt", false, Tone::Warning);
            return look;
        }
        let state = self.state.as_ref().expect("state");
        match action {
            Action::Overlay => {
                let on = state.flag("overlay").unwrap_or(false);
                look.badge = self.on_off(on, "sd_z_an", "sd_z_aus");
            }
            Action::Main => {
                // ZU (white): a press opens; HINTEN (yellow): a press brings it to the front; OFFEN (green): a
                // press closes. The word says what the press does.
                let window = state.get("hauptfenster").and_then(Value::as_str).and_then(streamdeck::MainWindow::from_id);
                look.badge = match window.unwrap_or(streamdeck::MainWindow::Closed) {
                    streamdeck::MainWindow::Closed => self.badge("sd_z_zu", false, Tone::Muted),
                    streamdeck::MainWindow::Behind => self.badge("sd_z_hinten", true, Tone::Warning),
                    streamdeck::MainWindow::Front => self.badge("sd_z_offen", true, Tone::Accent),
                };
            }
            Action::AntiAfk => {
                let on = state.get("anti_afk").and_then(|v| v["an"].as_bool()).unwrap_or(false);
                look.badge = self.on_off(on, "sd_z_an", "sd_z_aus");
            }
            Action::MiningScanner | Action::ClickThrough | Action::Sounds => {
                let on = state.get(action.id()).and_then(|v| v["an"].as_bool()).unwrap_or(false);
                look.badge = self.on_off(on, "sd_z_an", "sd_z_aus");
            }
            Action::Refinery => {
                let jobs = state.get("raffinerie");
                let count = jobs.and_then(|j| j["anzahl"].as_u64()).unwrap_or(0);
                let done = jobs.and_then(|j| j["fertig"].as_u64()).unwrap_or(0);
                let next = jobs.and_then(|j| j["naechste"].as_u64());
                if count == 0 {
                    look.badge = self.badge("sd_z_keine", false, Tone::Muted);
                } else if done > 0 {
                    look.badge = self.badge("sd_z_fertig", true, Tone::Accent);
                } else if let Some(seconds) = next {
                    look.label = language::tf_in(self.lang(), "sd_z_bis", &[("0", duration(seconds).into())]);
                    look.badge = Some(Badge {
                        word: language::tf_in(self.lang(), "sd_z_auftraege", &[("0", count.to_string().into())]),
                        filled: false,
                        tone: Tone::Muted,
                    });
                }
            }
            Action::Page => {
                let page = key.settings["seite"].as_str().unwrap_or("");
                if page.is_empty() {
                    look.badge = self.badge("sd_ga_waehlen", false, Tone::Muted);
                } else if streamdeck::page_allowed(page) {
                    look.label = self.t(page);
                    look.icon = page_icon(page);
                } else {
                    look.icon = "triangle-alert";
                    look.icon_tone = Tone::Error;
                    look.label = self.t("sd_z_unbekannt");
                }
            }
            Action::Contested => match state.get("contested") {
                Some(cycle) => {
                    // The name stays; below it the time left and the state: „55 min · OFFEN“.
                    let open = cycle["offen"].as_bool().unwrap_or(false);
                    let seconds = cycle["sekunden"].as_u64().unwrap_or(0);
                    look.badge = self
                        .on_off(open, "sd_z_offen", "sd_z_zu")
                        .map(|b| Badge { word: format!("{} · {}", duration(seconds), b.word), ..b });
                }
                None => look.badge = self.badge("sd_z_keine", false, Tone::Muted),
            },
            Action::Wallet => look.badge = self.badge("sd_z_ab", false, Tone::Muted),
            Action::Health => match streamdeck::health_of(state) {
                Some(percent) => {
                    let (_, tone) = health_level(percent);
                    look.badge = Some(Badge { word: format!("{percent} %"), filled: tone == Tone::Accent, tone });
                }
                None => look.badge = self.badge("sd_z_ab", false, Tone::Muted),
            },
            Action::GameAction | Action::Emote => self.game_look(key, state, &mut look),
            _ => {}
        }
        look
    }

    /// The look of a counted switch: on or off, with a question mark while it is only counted.
    fn counted_look(&self, context: &str, look: &mut Look) {
        let counted = self.counted.get(context).copied().unwrap_or_default();
        let word = match (counted.on, counted.sure) {
            (true, true) => "sd_z_an",
            (false, true) => "sd_z_aus",
            (true, false) => "sd_z_an_unsicher",
            (false, false) => "sd_z_aus_unsicher",
        };
        let tone = if counted.sure { Tone::Accent } else { Tone::Warning };
        look.badge = Some(Badge { word: self.t(word), filled: counted.on, tone: if counted.on { tone } else { Tone::Muted } });
    }

    /// What a key shows now, by its context (a counted switch shows its count).
    pub fn look_of(&self, context: &str) -> Option<Look> {
        let key = self.keys.get(context)?;
        let mut look = self.look(key);
        let usable = self.missing.is_none() && self.state.is_some() && look.icon_tone != Tone::Warning;
        if Self::counts(key) && usable {
            self.counted_look(context, &mut look);
        }
        Some(look)
    }

    fn on_off(&self, on: bool, on_key: &str, off_key: &str) -> Option<Badge> {
        if on { self.badge(on_key, true, Tone::Accent) } else { self.badge(off_key, false, Tone::Muted) }
    }

    /// The game actions a key presses: one on a key, up to three on a dial (left, right, push).
    fn game_actions(key: &Key) -> Vec<String> {
        let names: &[&str] = match key.controller {
            Controller::Keypad => &["aktion"],
            Controller::Encoder => &["aktion_links", "aktion_rechts", "aktion_druck"],
        };
        names.iter().filter_map(|n| key.settings[*n].as_str()).filter(|s| !s.is_empty()).map(String::from).collect()
    }

    /// Why a game action cannot be sent right now: the text key and the symbol; `None` when it can.
    fn game_block(&self, action: &str, state: &State) -> Option<(&'static str, &'static str)> {
        match state.get("spielaktion").and_then(|v| v["gesperrt"].as_str()) {
            Some("admin") => return Some(("sd_ga_admin", "shield-alert")),
            Some("spiel_admin") => return Some(("sd_ga_spiel_admin", "shield-alert")),
            _ => {}
        }
        let Some(binding) = self.bindings.get(action) else { return Some(("sd_ga_unbekannt", "circle-question-mark")) };
        match binding.kind.as_str() {
            "tastatur" => None,
            "nur_maus" => Some(("sd_ga_nur_maus", "mouse")),
            "nur_joystick" => Some(("sd_ga_nur_joystick", "joystick")),
            "nur_gamepad" => Some(("sd_ga_nur_gamepad", "gamepad-2")),
            "nicht_belegt" => Some(("sd_ga_nicht_belegt", "keyboard-off")),
            "nicht_sendbar" => Some(("sd_ga_nicht_sendbar", "triangle-alert")),
            _ => Some(("sd_ga_unbekannt", "circle-question-mark")),
        }
    }

    /// A game action that cannot be sent was pressed: the key says so; where its binding on the keyboard is missing,
    /// VerseKit opens its page of the keys, where the missing keys are put in, so a key is never silently dead.
    fn refused_game(&mut self, context: &str, reason: &str, verse: &mut dyn Verse) -> Vec<Value> {
        if MISSING_BINDING.contains(&reason) {
            let _ = verse.command(&Command::Page(streamdeck::PAGE_KEYS.into()));
        }
        vec![Self::alert(context)]
    }

    /// What a key of the profiles names itself: the short name (`name`), the symbol (`symbol`, one of the set) and
    /// the colour of its category (`farbe`). What it does not name, or names wrongly, stays as the action draws it.
    fn profile_look(key: &Key, look: &mut Look) {
        let setting = |name: &str| key.settings[name].as_str().map(str::trim).filter(|v| !v.is_empty());
        if let Some(name) = setting("name") {
            look.label = name.to_string();
        }
        if let Some(icon) = setting("symbol").and_then(crate::render::known_icon) {
            look.icon = icon;
        }
        if let Some(colour) = setting("farbe").and_then(|c| crate::paint::style().key_colour(c)) {
            look.colour = Some(colour);
        }
    }

    /// The symbol and the colour of the first game action of a key, as the profiles show that action or by its
    /// `actionmap`; the key keeps its own where it names one ([`Plugin::profile_look`] comes after).
    fn game_style(&self, key: &Key, look: &mut Look) {
        let Some(first) = Self::game_actions(key).into_iter().next() else { return };
        let map = self.bindings.get(&first).map_or("", |b| b.map.as_str());
        if let Some(style) = crate::game_style::of(&first, map) {
            look.icon = style.icon;
            look.colour = Some(style.colour);
        }
    }

    fn game_look(&self, key: &Key, state: &State, look: &mut Look) {
        let actions = Self::game_actions(key);
        let Some(first) = actions.first() else {
            look.badge = self.badge("sd_ga_waehlen", false, Tone::Muted);
            return;
        };
        // The long name of the game only where the key brings no short name of its own.
        let short = key.settings["name"].as_str().is_some_and(|n| !n.trim().is_empty());
        if let Some(binding) = self.bindings.get(first)
            && !short
        {
            look.label = binding.name.clone();
        }
        let blocked = actions.iter().find_map(|a| self.game_block(a, state));
        match blocked {
            Some((text, icon)) => {
                look.icon = icon;
                look.icon_tone = Tone::Warning;
                look.badge = Some(Badge { word: self.t(text), filled: false, tone: Tone::Warning });
            }
            // A dangerous game action says that it must be held, instead of its key.
            None if Self::dangerous(key) => look.badge = self.badge("sd_z_halten", false, Tone::Muted),
            // A key held with Alt Gr: the game may press the key alone, so the key says so in the colour of warnings.
            None if actions.iter().any(|a| self.bindings.get(a).is_some_and(|b| b.unsafe_key)) => {
                look.icon_tone = Tone::Warning;
                look.badge = self.badge("sd_ga_unsicher", false, Tone::Warning);
            }
            // The key of the game only where the key asks for it (`taste_anzeigen`); without it the name stands larger.
            None => {
                let wanted = key.settings["taste_anzeigen"].as_bool().unwrap_or(false);
                let shown = self.bindings.get(first).map(|b| b.key.clone()).unwrap_or_default();
                if wanted && !shown.is_empty() {
                    look.badge = Some(Badge { word: shown, filled: false, tone: Tone::Muted });
                }
            }
        }
    }

    fn alert(context: &str) -> Value {
        event("showAlert", context, Value::Null)
    }

    /// A key pressed.
    fn press(&mut self, context: &str, verse: &mut dyn Verse) -> Vec<Value> {
        let Some(key) = self.keys.get(context).cloned() else { return Vec::new() };
        if self.missing.is_some() || self.state.is_none() {
            return vec![Self::alert(context)];
        }
        let command = match key.action {
            Action::Overlay => Command::Overlay,
            Action::Main => Command::Main,
            Action::RefineryScan => Command::RefineryScan,
            Action::Blueprint => Command::Blueprint,
            Action::Stock => Command::Stock,
            Action::UnitStock => Command::UnitStock,
            Action::Search => Command::Search,
            Action::Picture => Command::Picture,
            Action::AntiAfk => Command::AntiAfk,
            Action::MiningScanner => Command::MiningScanner,
            Action::ClickThrough => Command::ClickThrough,
            Action::Sounds => Command::Sounds,
            Action::Refinery => Command::Page("hf_bergbau".into()),
            Action::Contested => Command::Page(streamdeck::PAGE_CONTESTED.into()),
            Action::Wallet => Command::Page(streamdeck::PAGE_FINANCES.into()),
            Action::Objective => Command::Page(PAGE_CONTRACT_LOG.into()),
            Action::Status => Command::Page(streamdeck::PAGE_KEYS.into()),
            Action::Ship => Command::Page(streamdeck::PAGE_SHIPS.into()),
            // Only a display: a press sends nothing to VerseKit and nothing to the game.
            Action::Health => return Vec::new(),
            Action::Page => {
                let page = key.settings["seite"].as_str().unwrap_or("");
                if !streamdeck::page_allowed(page) {
                    return vec![Self::alert(context)];
                }
                Command::Page(page.to_string())
            }
            Action::BindingsFolder => Command::BindingsFolder,
            Action::GameAction | Action::Emote => {
                let Some(action) = Self::game_actions(&key).into_iter().next() else { return vec![Self::alert(context)] };
                let now = self.now();
                if Self::dangerous(&key) {
                    return self.start_guard(context, &action, verse);
                }
                if Self::counts(&key) {
                    // A counted switch is sent when the key comes up: held long, it is set back instead.
                    if let Some((reason, _)) = self.state.as_ref().and_then(|s| self.game_block(&action, s)) {
                        return self.refused_game(context, reason, verse);
                    }
                    self.pressed.insert(context.to_string(), now);
                    return Vec::new();
                }
                if key.action == Action::Emote {
                    self.pressed.insert(context.to_string(), now);
                }
                return self.game_down(context, &action, verse);
            }
        };
        match verse.command(&command) {
            Ok(())
                if matches!(
                    key.action,
                    Action::Overlay | Action::Main | Action::AntiAfk | Action::MiningScanner | Action::ClickThrough | Action::Sounds
                ) =>
            {
                let mut out = Vec::new();
                out.extend(self.poll(verse));
                out
            }
            Ok(()) => vec![event("showOk", context, Value::Null)],
            Err(_) => vec![Self::alert(context)],
        }
    }

    fn game_down(&mut self, context: &str, action: &str, verse: &mut dyn Verse) -> Vec<Value> {
        if let Some((reason, _)) = self.state.as_ref().and_then(|s| self.game_block(action, s)) {
            return self.refused_game(context, reason, verse);
        }
        match verse.command(&Command::GameAction { action: action.to_string(), phase: Phase::Down }) {
            Ok(()) => {
                self.held.insert(context.to_string(), action.to_string());
                Vec::new()
            }
            Err(_) => vec![Self::alert(context)],
        }
    }

    /// A dangerous game action: sent only after it was held for [`HOLD`]. The profile says so (`gefaehrlich`); where
    /// it says nothing, the action pressed decides ([`streamdeck::dangerous_by_itself`]: on a key its action, on a
    /// dial the one of the push).
    fn dangerous(key: &Key) -> bool {
        if key.action != Action::GameAction {
            return false;
        }
        if let Some(said) = key.settings["gefaehrlich"].as_bool() {
            return said;
        }
        let pressed = match key.controller {
            Controller::Keypad => "aktion",
            Controller::Encoder => "aktion_druck",
        };
        key.settings[pressed].as_str().is_some_and(streamdeck::dangerous_by_itself)
    }

    /// A dangerous game action pressed: nothing is sent yet, the ring on the key begins to fill.
    fn start_guard(&mut self, context: &str, action: &str, verse: &mut dyn Verse) -> Vec<Value> {
        if let Some((reason, _)) = self.state.as_ref().and_then(|s| self.game_block(action, s)) {
            return self.refused_game(context, reason, verse);
        }
        let now = self.now();
        self.hold_hint.remove(context);
        self.guarded.insert(context.to_string(), Guarded { action: action.to_string(), since: now, sent: false, drawn: now });
        self.draw(context)
    }

    /// How far the ring of a dangerous game action held down is filled (0 to 1); `None` where none is held.
    fn hold_progress(&self, context: &str) -> Option<f32> {
        let guard = self.guarded.get(context)?;
        let held = self.now().saturating_duration_since(guard.since);
        Some((held.as_secs_f32() / HOLD.as_secs_f32()).min(1.0))
    }

    /// The dangerous game actions held down: once held for [`HOLD`] the action goes to the game (and is let go with
    /// the key); until then the ring is drawn anew, not more often than every [`DIAL_FRAME`].
    fn follow_guards(&mut self, verse: &mut dyn Verse) -> Vec<Value> {
        let now = self.now();
        let mut out = Vec::new();
        let contexts: Vec<String> = self.guarded.keys().cloned().collect();
        for context in contexts {
            let Some(guard) = self.guarded.get(&context).cloned() else { continue };
            if guard.sent {
                continue;
            }
            if now.saturating_duration_since(guard.since) >= HOLD {
                if let Some(g) = self.guarded.get_mut(&context) {
                    g.sent = true;
                    g.drawn = now;
                }
                out.extend(self.game_down(&context, &guard.action, verse));
                out.extend(self.draw(&context));
            } else if now.saturating_duration_since(guard.drawn) >= DIAL_FRAME {
                if let Some(g) = self.guarded.get_mut(&context) {
                    g.drawn = now;
                }
                out.extend(self.draw(&context));
            }
        }
        let ended: Vec<String> = self.hold_hint.iter().filter(|(_, until)| **until <= now).map(|(c, _)| c.clone()).collect();
        for context in ended {
            self.hold_hint.remove(&context);
            out.extend(self.draw(&context));
        }
        out
    }

    /// A key let go: a game action held by it is released; a counted switch is switched or set back; an emote
    /// tapped goes back to its page; a dangerous game action let go too early sends nothing and says „HALTEN“.
    fn release(&mut self, context: &str, verse: &mut dyn Verse) -> Vec<Value> {
        if let Some(guard) = self.guarded.remove(context)
            && !guard.sent
        {
            let now = self.now();
            self.hold_hint.insert(context.to_string(), now + HOLD_HINT);
            let mut out = vec![Self::alert(context)];
            out.extend(self.draw(context));
            return out;
        }
        let held_for = self.pressed.remove(context).map(|at| self.now().saturating_duration_since(at));
        let key = self.keys.get(context).cloned();
        if let (Some(key), Some(held_for)) = (&key, held_for)
            && Self::counts(key)
        {
            return self.switch_counted(context, key, held_for, verse);
        }
        let mut out = Vec::new();
        if let Some(action) = self.held.remove(context)
            && verse.command(&Command::GameAction { action, phase: Phase::Up }).is_err()
        {
            return vec![Self::alert(context)];
        }
        if let (Some(key), Some(held_for)) = (&key, held_for)
            && key.action == Action::Emote
            && held_for < EMOTE_TAP
        {
            out.extend(self.back_to_page(key));
        }
        out
    }

    fn switch_counted(&mut self, context: &str, key: &Key, held_for: Duration, verse: &mut dyn Verse) -> Vec<Value> {
        if held_for >= SWITCH_RESET {
            self.counted.insert(context.to_string(), Counted { on: false, sure: true });
            let mut out = vec![event("showOk", context, Value::Null)];
            out.extend(self.draw(context));
            return out;
        }
        let Some(action) = Self::game_actions(key).into_iter().next() else { return vec![Self::alert(context)] };
        let down = verse.command(&Command::GameAction { action: action.clone(), phase: Phase::Down });
        let up = verse.command(&Command::GameAction { action, phase: Phase::Up });
        if down.is_err() || up.is_err() {
            return vec![Self::alert(context)];
        }
        let before = self.counted.get(context).copied().unwrap_or_default();
        self.counted.insert(context.to_string(), Counted { on: !before.on, sure: false });
        self.draw(context)
    }

    /// The switch to the page an emote key goes back to, in the shipped profile of its device; nothing when the
    /// key names no page or the device has no shipped profile.
    fn back_to_page(&self, key: &Key) -> Vec<Value> {
        let Some(page) = key.settings["zurueck_seite"].as_u64() else { return Vec::new() };
        let Some(device) = self.devices.iter().find(|d| d.id == key.device) else { return Vec::new() };
        let Some(base) = profile_base(device.kind) else { return Vec::new() };
        let profile = format!("{base}-{}", if self.lang() == Lang::En { "EN" } else { "DE" });
        if !self.profiles.contains(&profile) {
            return Vec::new();
        }
        vec![json!({"event": "switchToProfile", "context": self.uuid, "device": device.id, "payload": {"profile": profile_name(&profile), "page": page}})]
    }

    /// A new wish of VerseKit for a page of the device: every device with a shipped profile switches to that page of
    /// it, once per wish. The wish VerseKit holds when it is reached anew (or first) is an old one and passed over.
    fn follow_page_wish(&mut self, was_away: bool) -> Vec<Value> {
        let Some((nr, page)) = self.state.as_ref().and_then(streamdeck::device_page_of) else { return Vec::new() };
        if self.page_wish == Some(nr) {
            return Vec::new();
        }
        self.page_wish = Some(nr);
        if was_away {
            return Vec::new();
        }
        let suffix = if self.lang() == Lang::En { "EN" } else { "DE" };
        let mut out = Vec::new();
        for device in &self.devices {
            let Some(base) = profile_base(device.kind) else { continue };
            let profile = format!("{base}-{suffix}");
            let Some(number) = self.profiles.contains(&profile).then(|| self.pages.get(&profile)).flatten().and_then(|p| page.number_in(p))
            else {
                continue;
            };
            out.push(json!({"event": "switchToProfile", "context": self.uuid, "device": device.id, "payload": {"profile": profile_name(&profile), "page": number}}));
            out.push(note(&format!("Seite {} ({number}) für Gerät {}", page.id(), device.id)));
        }
        out
    }

    fn dial_press(&mut self, context: &str, phase: Phase, verse: &mut dyn Verse) -> Vec<Value> {
        let Some(key) = self.keys.get(context).cloned() else { return Vec::new() };
        if phase == Phase::Up {
            return self.release(context, verse);
        }
        let push = key.settings["aktion_druck"].as_str().unwrap_or("").to_string();
        if Self::dangerous(&key) && !push.is_empty() {
            return self.start_guard(context, &push, verse);
        }
        let mut out = if !matches!(key.action, Action::GameAction | Action::Emote) || push.is_empty() {
            self.press(context, verse)
        } else {
            self.game_down(context, &push, verse)
        };
        out.extend(self.start_motion(context, crate::render::press_frames()));
        out
    }

    /// The objective dial turned: one running contract further per step, round at the ends.
    fn turn_contracts(&mut self, context: &str, ticks: i64) -> Vec<Value> {
        let count = self.state.as_ref().map(|s| streamdeck::contracts_of(s).len()).unwrap_or(0);
        if count < 2 {
            return Vec::new();
        }
        let now = self.contract_at.get(context).copied().unwrap_or(0).min(count - 1) as i64;
        let next = (now + ticks).rem_euclid(count as i64) as usize;
        self.contract_at.insert(context.to_string(), next);
        self.draw(context)
    }

    /// Starts the short series of pictures of a dial (a new turn or press starts it anew) and draws its first.
    fn start_motion(&mut self, context: &str, frames: Vec<crate::render::Motion>) -> Vec<Value> {
        let now = self.now();
        self.motion.insert(context.to_string(), (frames, 0, now + DIAL_FRAME));
        self.draw(context)
    }

    /// Is a dial in the middle of its series, or waiting for the limit? Then [`Plugin::tick`] wants to be called
    /// every [`DIAL_FRAME`].
    pub fn busy(&self) -> bool {
        !self.motion.is_empty() || !self.dial_waiting.is_empty() || !self.guarded.is_empty() || !self.hold_hint.is_empty()
    }

    /// The next pictures of the dials whose time has come, the pictures that waited for the limit, and the dangerous
    /// game actions held down.
    pub fn tick(&mut self, verse: &mut dyn Verse) -> Vec<Value> {
        let now = self.now();
        let mut out = self.follow_guards(verse);
        let due: Vec<String> = self.motion.iter().filter(|(_, (_, _, at))| *at <= now).map(|(c, _)| c.clone()).collect();
        for context in due {
            let ended = match self.motion.get_mut(&context) {
                Some((frames, at, next)) => {
                    *at += 1;
                    *next = now + DIAL_FRAME;
                    *at >= frames.len()
                }
                None => continue,
            };
            if ended {
                self.motion.remove(&context);
            }
            out.extend(self.draw(&context));
        }
        let waiting: Vec<String> = self.dial_waiting.iter().cloned().collect();
        for context in waiting {
            out.extend(self.draw(&context));
        }
        out
    }

    /// A dial turned: the action of its side pressed once per step (at most ten).
    fn dial_turn(&mut self, context: &str, ticks: i64, verse: &mut dyn Verse) -> Vec<Value> {
        let Some(key) = self.keys.get(context).cloned() else { return Vec::new() };
        if key.action == Action::Objective {
            return self.turn_contracts(context, ticks);
        }
        if !matches!(key.action, Action::GameAction | Action::Emote) || ticks == 0 {
            return Vec::new();
        }
        let side = if ticks < 0 { "aktion_links" } else { "aktion_rechts" };
        let Some(action) = key.settings[side].as_str().filter(|s| !s.is_empty()).map(String::from) else {
            return vec![Self::alert(context)];
        };
        if let Some((reason, _)) = self.state.as_ref().and_then(|s| self.game_block(&action, s)) {
            return self.refused_game(context, reason, verse);
        }
        for _ in 0..ticks.unsigned_abs().min(10) {
            let down = verse.command(&Command::GameAction { action: action.clone(), phase: Phase::Down });
            let up = verse.command(&Command::GameAction { action: action.clone(), phase: Phase::Up });
            if down.is_err() || up.is_err() {
                return vec![Self::alert(context)];
            }
        }
        self.start_motion(context, crate::render::turn_frames(if ticks < 0 { -1 } else { 1 }))
    }

    /// Tells VerseKit which game actions lie on the keys, when that changed.
    fn report_game_actions(&mut self, verse: &mut dyn Verse) -> Vec<Value> {
        if self.missing.is_some() || self.state.is_none() {
            return Vec::new();
        }
        // The keys shown now, the keys shown before in this run (other pages of the player's own profile), and every
        // page of the shipped profile of each device: a page not shown yet counts too.
        for key in self.keys.values().filter(|k| matches!(k.action, Action::GameAction | Action::Emote)) {
            for name in Self::game_actions(key) {
                if !self.seen_actions.contains(&name) {
                    self.seen_actions.push(name);
                }
            }
        }
        let suffix = if self.lang() == Lang::En { "EN" } else { "DE" };
        let shipped = self
            .devices
            .iter()
            .filter_map(|d| profile_base(d.kind))
            .filter_map(|base| self.profile_actions.get(&format!("{base}-{suffix}")))
            .flatten();
        let mut names: Vec<String> = Vec::new();
        for name in self.seen_actions.iter().chain(shipped) {
            if !names.contains(name) && streamdeck::plausible_action_name(name) {
                names.push(name.clone());
            }
        }
        names.sort();
        names.truncate(streamdeck::MAX_REPORTED_ACTIONS);
        // The body of a command has a limit of its own: the last names go until it fits.
        while !names.is_empty()
            && streamdeck::command_body(&Command::GameActionsInUse(names.clone())).len() > streamdeck::MAX_BODY
        {
            names.pop();
        }
        if names != self.reported && verse.command(&Command::GameActionsInUse(names.clone())).is_ok() {
            self.reported = names;
        }
        Vec::new()
    }

    /// What the property inspector of a key needs: the texts, the pages, the game actions.
    fn inspector(&self, context: &str) -> Vec<Value> {
        let Some(key) = self.keys.get(context) else { return Vec::new() };
        let lang = self.lang();
        let pages: Vec<Value> =
            streamdeck::PAGES.iter().map(|p| json!({"id": p, "name": language::t_in(lang, p)})).collect();
        let mut actions: Vec<(&String, &Binding)> = self.bindings.iter().collect();
        actions.sort_by(|a, b| (a.1.group.as_str(), a.1.name.as_str()).cmp(&(b.1.group.as_str(), b.1.name.as_str())));
        // An emote key chooses among the emotes; without the actionmaps (an older VerseKit) among all.
        let emotes = key.action == Action::Emote && actions.iter().any(|(_, b)| b.map == EMOTE_MAP);
        let chosen: Vec<String> = Self::game_actions(key);
        let actions: Vec<Value> = actions
            .into_iter()
            .filter(|(id, b)| !emotes || b.map == EMOTE_MAP || chosen.contains(id))
            .map(|(id, b)| {
                let sendable = b.kind == "tastatur";
                // Why it cannot be pressed, in the words the key would say.
                let hint = if sendable { String::new() } else { self.t(&format!("sd_ga_{}", b.kind)) };
                json!({"id": id, "name": b.name, "gruppe": b.group, "art": b.kind, "taste": b.key,
                       "sendbar": sendable, "hinweis": hint})
            })
            .collect();
        let texts: serde_json::Map<String, Value> = [
            "sd_pi_seite",
            "sd_pi_aktion",
            "sd_pi_suchen",
            "sd_pi_taste",
            "sd_pi_belegen",
            "sd_pi_steuerung",
            "sd_pi_getrennt",
            "sd_pi_keine_aktionen",
            "sd_pi_links",
            "sd_pi_rechts",
            "sd_pi_druck",
            "sd_ga_nur_maus",
            "sd_ga_nur_joystick",
            "sd_ga_nur_gamepad",
            "sd_ga_nicht_belegt",
            "sd_ga_nicht_sendbar",
            "sd_ga_unbekannt",
            "sd_ga_waehlen",
            "sd_pi_umschalter",
            "sd_pi_umschalter_h",
            "sd_pi_taste_zeigen",
            "sd_pi_taste_zeigen_h",
            "sd_pi_zurueck",
            "sd_pi_zurueck_h",
            "sd_pi_zurueck_bsp",
            "sd_pi_seite_waehlen",
            "sd_pi_nur_sendbar",
            "sd_pi_nur_sendbar_h",
        ]
        .iter()
        .map(|k| (k.to_string(), Value::from(language::t_in(lang, k))))
        .collect();
        let payload = json!({
            "sprache": lang.code(),
            "verbunden": self.missing.is_none() && self.state.is_some(),
            "aktion": key.action.id(),
            "regler": key.controller == Controller::Encoder,
            "texte": texts,
            "seiten": pages,
            "aktionen": actions,
            "einstellungen": key.settings,
        });
        vec![json!({"event": "sendToPropertyInspector", "context": context, "action": key.action.uuid(), "payload": payload})]
    }

    fn inspector_asked(&mut self, context: &str, payload: &Value, verse: &mut dyn Verse) -> Vec<Value> {
        match payload["frage"].as_str() {
            Some("daten") => self.inspector(context),
            Some("steuerung") => match verse.command(&Command::Page(streamdeck::PAGE_CONTROLS.into())) {
                Ok(()) => Vec::new(),
                Err(_) => vec![Self::alert(context)],
            },
            _ => Vec::new(),
        }
    }

    /// Offers the shipped profile in the language of VerseKit once per device; says when a device has none.
    fn offer_profiles(&mut self, verse: &mut dyn Verse) -> Vec<Value> {
        if !self.global_read || self.state.is_none() || self.missing.is_some() {
            return Vec::new();
        }
        let suffix = if self.lang() == Lang::En { "EN" } else { "DE" };
        let mut out = Vec::new();
        let mut hint: Option<String> = None;
        let mut changed = false;
        for device in self.devices.clone() {
            let Some(base) = profile_base(device.kind) else {
                hint = Some(device.name.clone());
                continue;
            };
            let profile = format!("{base}-{suffix}");
            if !self.profiles.contains(&profile) {
                hint = Some(device.name.clone());
                continue;
            }
            if self.offered.contains(&device.id) {
                continue;
            }
            self.offered.push(device.id.clone());
            changed = true;
            out.push(json!({"event": "switchToProfile", "context": self.uuid, "device": device.id, "payload": {"profile": profile_name(&profile)}}));
            out.push(note(&format!("switchToProfile {} für Gerät {}", profile_name(&profile), device.id)));
        }
        if changed {
            out.push(json!({"event": "setGlobalSettings", "context": self.uuid, "payload": {"profil_angeboten": self.offered}}));
        }
        if self.told_hint.as_ref() != Some(&hint) && verse.command(&Command::DeviceHint(hint.clone())).is_ok() {
            self.told_hint = Some(hint);
        }
        out
    }

    /// Lets go of every held game action (the plugin ends).
    pub fn release_all(&mut self, verse: &mut dyn Verse) {
        let contexts: Vec<String> = self.held.keys().cloned().collect();
        for context in contexts {
            let _ = self.release(&context, verse);
        }
    }
}

/// The word and the tone of a health in per cent: good (green) from 60, hurt (yellow) from 30, critical (red) below.
pub fn health_level(percent: u8) -> (&'static str, Tone) {
    match percent {
        60.. => ("sd_info_gesund", Tone::Accent),
        30..=59 => ("sd_info_angeschlagen", Tone::Warning),
        _ => ("sd_info_kritisch", Tone::Error),
    }
}

/// Seconds as `1:05 h`, `12 min` or `45 s`.
pub fn duration(seconds: u64) -> String {
    if seconds >= 3600 {
        format!("{}:{:02} h", seconds / 3600, (seconds % 3600) / 60)
    } else if seconds >= 60 {
        format!("{} min", seconds.div_ceil(60))
    } else {
        format!("{seconds} s")
    }
}

#[cfg(test)]
mod tests;
