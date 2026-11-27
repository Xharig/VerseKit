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

//! The overlay's side of the Stream Deck interface: what the state holds, what a command does. A command takes the
//! way of the hotkey it stands for (the same channel), a page and the main window go to the overlay as orders, and
//! a game action is pressed here ([`GameKeys`]).
//!
//! The bindings of the game actions are worked out in a thread of their own (reading the names of the game takes
//! a moment) whenever the `actionmaps.xml` or the language changed; until then the state says the old mark.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use serde_json::{Map, Value, json};
use versekit_core::game_binding::{self, How, Resolved};
use versekit_core::language;
use versekit_core::paths::{Host as SystemHost, Paths};
use versekit_core::streamdeck::{self, Command, KeyFile, Phase};
use versekit_core::streamdeck_keys::{GameKeys, KeySink, NotSent, SystemKeys};
use versekit_core::streamdeck_server::{self, Host, Refused, Server};
use versekit_core::theme;
use versekit_core::window_signal::{self, Role};

/// What the overlay shows, written by the overlay in every frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Snapshot {
    pub overlay_open: bool,
    pub anti_afk: bool,
    pub mining_scanner: bool,
    pub click_through: bool,
    pub sounds: bool,
    /// The game runs.
    pub game_runs: bool,
    /// The watch of the log found the running log; `None` while it has not looked yet.
    pub log_found: Option<bool>,
}

/// What only the overlay itself can do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Order {
    /// The main window: started, brought to the front, or closed when it is in front.
    Main,
    /// A page of the main window.
    Page(String),
    /// A folder shown in the file manager; the host names it, never the plugin.
    Folder(String),
    /// Clicking through the overlay switched over, as the lock of the bar does.
    ClickThrough,
    /// The sounds of the overlay switched over.
    Sounds,
    /// The plugin file beside VerseKit opened in the Stream Deck program, which installs it over the older plugin
    /// (asking or not). The overlay waits with it while the game runs.
    PluginFile(String),
}

/// The channels of the hotkeys a command takes.
pub struct Senders {
    pub overlay: Sender<()>,
    pub refinery: Sender<()>,
    pub picture: Sender<()>,
    pub search: Sender<()>,
    pub anti_afk: Sender<()>,
    pub blueprint: Sender<()>,
    pub stock: Sender<()>,
    pub unit_stock: Sender<()>,
    /// The channel of the hotkey of the mining scanner (the eye of the bar does the same).
    pub mining_scanner: Sender<()>,
}

/// What the overlay hands over to start the interface later: the channels and how to wake it.
pub struct DeckParts {
    pub senders: Senders,
    pub wake: Arc<dyn Fn() + Send + Sync>,
}

/// The bindings of the game actions as last worked out.
#[derive(Default)]
struct Bindings {
    mark: String,
    list: Vec<Resolved>,
    names: HashMap<String, String>,
    groups: HashMap<String, String>,
    /// The actions that keep a mouse button the deck cannot press, because nothing can move without a loss: they work
    /// with the mouse, the deck says „Nur Maus“, and they count as no missing key.
    mouse_only: std::collections::HashSet<String>,
    /// The game actions on the deck that are missing a key ([`versekit_core::streamdeck_fill::missing`]).
    missing: std::collections::HashSet<String>,
    /// How the game takes the key of each action of the list that does not simply follow it.
    timings: HashMap<String, versekit_core::controls::Timing>,
}

/// Values read from files, kept for a while.
#[derive(Default)]
struct Cache {
    refinery: Option<(Instant, Value)>,
    contested: Option<(Instant, Option<i64>)>,
}

const REFINERY_EVERY: Duration = Duration::from_secs(5);
const CONTESTED_EVERY: Duration = Duration::from_secs(60);

pub struct DeckHost {
    paths: Paths,
    snapshot: Arc<Mutex<Snapshot>>,
    senders: Mutex<Senders>,
    orders: Mutex<Sender<Order>>,
    wake: Arc<dyn Fn() + Send + Sync>,
    cache: Mutex<Cache>,
    bindings: Arc<Mutex<Bindings>>,
    working: Arc<AtomicBool>,
    /// The device the plugin found no profile for.
    pub hint: Mutex<Option<String>>,
    /// The game actions on the keys of the plugin.
    pub in_use: Mutex<Vec<String>>,
    /// The version the installed plugin told.
    pub plugin_version: Mutex<Option<String>>,
    /// The running contracts of the bar, as the state carries them.
    contracts: Mutex<Value>,
    /// The field [`streamdeck::FIELD_SHIP`].
    ship: Mutex<Value>,
    /// The newest wish for a page of the device and its number.
    device_page: Mutex<Option<(u64, streamdeck::DevicePage)>>,
    pub keys: GameKeys,
    sink: Box<dyn KeySink>,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

impl DeckHost {
    pub fn new(
        paths: Paths,
        snapshot: Arc<Mutex<Snapshot>>,
        senders: Senders,
        orders: Sender<Order>,
        wake: Arc<dyn Fn() + Send + Sync>,
        sink: Box<dyn KeySink>,
    ) -> DeckHost {
        DeckHost {
            paths,
            snapshot,
            senders: Mutex::new(senders),
            orders: Mutex::new(orders),
            wake,
            cache: Mutex::default(),
            bindings: Arc::default(),
            working: Arc::default(),
            hint: Mutex::default(),
            in_use: Mutex::default(),
            plugin_version: Mutex::default(),
            contracts: Mutex::new(Value::Array(Vec::new())),
            ship: Mutex::new(Value::Null),
            device_page: Mutex::default(),
            keys: GameKeys::default(),
            sink,
        }
    }

    /// Whether all is ready: the game, the log, and how many game actions on the deck have no key on the keyboard
    /// (none counted while the bindings are not worked out yet).
    fn readiness(&self, snapshot: &Snapshot) -> streamdeck::Readiness {
        let bindings = lock(&self.bindings);
        // The same rule as the bar of the main window ([`streamdeck_fill::missing`]), worked out with the bindings.
        let keys_missing = lock(&self.in_use).iter().filter(|name| bindings.missing.contains(*name)).count();
        streamdeck::Readiness { game_runs: snapshot.game_runs, log_found: snapshot.log_found, keys_missing }
    }

    /// The running contracts of the bar for the objective dial; the plugin is woken only when they changed.
    pub fn set_contracts(&self, lines: &[versekit_core::live_contracts::Line]) {
        let value = streamdeck::contracts_value(lines);
        let mut contracts = lock(&self.contracts);
        if *contracts != value {
            *contracts = value;
            drop(contracts);
            (self.wake)();
        }
    }

    /// The readable name of the last ship of the running session, or none.
    pub fn set_ship(&self, name: Option<&str>) {
        let value = streamdeck::ship_value(name);
        let mut ship = lock(&self.ship);
        if *ship != value {
            *ship = value;
            drop(ship);
            (self.wake)();
        }
    }

    /// Asks the plugin for a page of the device: a wish with the next number, so it is switched once.
    pub fn ask_page(&self, page: streamdeck::DevicePage) {
        let mut wish = lock(&self.device_page);
        let nr = wish.map_or(1, |(nr, _)| nr + 1);
        *wish = Some((nr, page));
        drop(wish);
        (self.wake)();
    }

    /// The mark the bindings would have now: the state of the `actionmaps.xml`, the language and the game actions on
    /// the deck.
    fn wanted_mark(&self) -> String {
        format!(
            "{}|{}|{}",
            versekit_core::anti_afk_key::stamp(&self.paths),
            language::current().code(),
            lock(&self.in_use).join(",")
        )
    }


    /// Works the bindings out anew in a thread, when the mark changed and no thread is at it.
    fn refresh_bindings(&self) {
        let wanted = self.wanted_mark();
        if lock(&self.bindings).mark == wanted || self.working.swap(true, Ordering::SeqCst) {
            return;
        }
        let (paths, store, working, wake) =
            (Paths::new(self.paths.host.clone()), self.bindings.clone(), self.working.clone(), self.wake.clone());
        let lang = language::current();
        let in_use = lock(&self.in_use).clone();
        let spawned = std::thread::Builder::new().name("streamdeck-belegung".into()).spawn(move || {
            let fresh = language::with_language(lang, || work_out(&paths, wanted, &in_use));
            *lock(&store) = fresh;
            working.store(false, Ordering::SeqCst);
            wake();
        });
        if spawned.is_err() {
            self.working.store(false, Ordering::SeqCst);
        }
    }

    fn refinery(&self) -> Value {
        let mut cache = lock(&self.cache);
        if let Some((at, value)) = &cache.refinery
            && at.elapsed() < REFINERY_EVERY
        {
            return value.clone();
        }
        let now = versekit_core::contested::now();
        let (running, done) = versekit_core::refinery_jobs::current_jobs(&self.paths, now);
        let next = running
            .iter()
            .map(|job| versekit_core::refinery_jobs::ends(job) - now)
            .filter(|left| *left > 0.0)
            .fold(None, |min: Option<f64>, left| Some(min.map_or(left, |m| m.min(left))));
        let value = json!({
            "anzahl": running.len() + done.len(),
            "fertig": done.len(),
            "naechste": next.map(|s| s.ceil() as u64),
        });
        cache.refinery = Some((Instant::now(), value.clone()));
        value
    }

    fn contested(&self) -> Value {
        let start = {
            let mut cache = lock(&self.cache);
            match &cache.contested {
                Some((at, start)) if at.elapsed() < CONTESTED_EVERY => *start,
                _ => {
                    let start = versekit_core::contested::load(&self.paths).start();
                    cache.contested = Some((Instant::now(), start));
                    start
                }
            }
        };
        let Some(start) = start else { return Value::Null };
        let hangar = versekit_core::contested::hangar_at(start, versekit_core::contested::now());
        json!({
            "offen": hangar.phase == versekit_core::contested::Phase::Open,
            "sekunden": hangar.to_change.max(0),
        })
    }

    /// Why game actions cannot be pressed now: VerseKit or only the game with administrator rights.
    fn blocked(&self) -> Option<&'static str> {
        if versekit_core::anti_afk::self_elevated() == Some(true) {
            return Some("admin");
        }
        use versekit_core::anti_afk::Platform;
        (versekit_core::anti_afk::System.game_needs_admin() == Some(true)).then_some("spiel_admin")
    }

    fn send(&self, pick: impl Fn(&Senders) -> &Sender<()>) -> Result<(), Refused> {
        let senders = lock(&self.senders);
        pick(&senders).send(()).map_err(|_| Refused { status: 503, code: "nicht_bereit" })?;
        (self.wake)();
        Ok(())
    }

    /// The plugin file beside VerseKit and the version it names; `None` without one.
    fn shipped_plugin() -> Option<(String, String)> {
        let file = streamdeck::plugin_file()?;
        let version = streamdeck::plugin_file_version(&std::fs::read(&file).ok()?)?;
        Some((file, version))
    }

    /// The installed plugin told its version: when it is older than the plugin VerseKit brings, the file is opened
    /// once per version VerseKit brings (after an update of VerseKit). Not again when the player declined it in the
    /// Stream Deck program; the page of keys says the plugin is old and offers „Set up“ from then on.
    fn offer_newer_plugin(&self, installed: &str, shipped: Option<(String, String)>) -> Result<(), Refused> {
        let Some((file, version)) = shipped else { return Ok(()) };
        if !streamdeck::version_older(installed, &version)
            || self.paths.setting(streamdeck::SETTING_PLUGIN_OFFERED).as_deref() == Some(version.as_str())
        {
            return Ok(());
        }
        self.paths.set_setting(streamdeck::SETTING_PLUGIN_OFFERED, versekit_core::jsonio::Value::Str(version));
        self.order(Order::PluginFile(file))
    }

    /// The folder of the `actionmaps.xml` the game works with; `None` without one.
    fn bindings_folder(&self) -> Option<String> {
        let file = versekit_core::joysticks::actionmaps_path(&self.paths, None)?;
        let folder = std::path::Path::new(&file).parent()?;
        folder.is_dir().then(|| folder.to_string_lossy().into_owned())
    }

    fn order(&self, order: Order) -> Result<(), Refused> {
        lock(&self.orders).send(order).map_err(|_| Refused { status: 503, code: "nicht_bereit" })?;
        (self.wake)();
        Ok(())
    }

    fn game_action(&self, action: &str, phase: Phase) -> Result<(), Refused> {
        if phase == Phase::Up {
            self.keys.up(action, self.sink.as_ref(), Instant::now());
            return Ok(());
        }
        if let Some(why) = self.blocked() {
            return Err(Refused { status: 409, code: if why == "admin" { "admin" } else { "spiel_admin" } });
        }
        let (how, timing) = {
            let bindings = lock(&self.bindings);
            let how = bindings.list.iter().find(|r| r.action == action).map(|r| r.how.clone());
            (how, bindings.timings.get(action).copied().unwrap_or_default())
        };
        match how {
            None => Err(Refused { status: 400, code: "aktion" }),
            Some(How::Keyboard { keys, .. }) => match self.keys.down(action, &keys, timing, self.sink.as_ref(), Instant::now()) {
                Ok(()) => Ok(()),
                Err(NotSent::NotInFront) => Err(Refused { status: 409, code: "nicht_vorne" }),
                Err(NotSent::Failed) => Err(Refused { status: 503, code: "nicht_gesendet" }),
            },
            Some(_) => Err(Refused { status: 409, code: "nicht_sendbar" }),
        }
    }
}

/// Works out the bindings with the names of the game in the language of the program; `qualified` are names of keys
/// with an `actionmap` before a dot, each worked out besides the plain ones where the game knows it.
fn work_out(paths: &Paths, mark: String, in_use: &[String]) -> Bindings {
    let qualified: Vec<String> = in_use.iter().filter(|n| n.contains('.')).cloned().collect();
    let qualified = qualified.as_slice();
    let profile = versekit_gamedata::control_profile::profile(paths);
    let own = match versekit_core::joysticks::actionmaps_path(paths, None) {
        Some(file) => versekit_core::joysticks::bindings(paths, Some(&file), None),
        None => Vec::new(),
    };
    let list = with_qualified(game_binding::resolve_all(&own, &profile), qualified, &own, &profile);
    let lang = language::current();
    let folder = paths.game_folder();
    let (labels, groups) = match &folder {
        Some(folder) => versekit_core::controls::names_from_folder(&profile, folder, lang),
        None => Default::default(),
    };
    // Where the game has no text in the language of the program, the short name of the profiles comes first.
    let without_text = versekit_core::controls::without_own_text(&profile, folder.as_deref(), lang);
    let names = list
        .iter()
        .map(|r| {
            let plain = game_binding::qualified(&r.action, &profile).1;
            let game = labels.get(plain).map_or_else(
                || versekit_core::controls::technical_readable(plain),
                |label| label.name.clone(),
            );
            let own_text = !without_text.contains(plain) && labels.get(plain).is_some_and(|l| l.real);
            (r.action.clone(), streamdeck::game_action_name(plain, &game, own_text, lang))
        })
        .collect();
    let groups = list
        .iter()
        .map(|r| {
            let name = groups.get(&r.group).map_or_else(
                || versekit_core::controls::technical_readable(&r.group),
                |label| label.name.clone(),
            );
            (r.group.clone(), name)
        })
        .collect();
    // Only the actions on the deck: the rules look at the whole binding for each.
    use versekit_core::streamdeck_fill;
    let mouse_only = in_use.iter().filter(|name| streamdeck_fill::stays_on_mouse(name, &own, &profile)).cloned().collect();
    let missing = in_use.iter().filter(|name| streamdeck_fill::missing(name, &own, &profile)).cloned().collect();
    let timings = list
        .iter()
        .map(|r| (r.action.clone(), profile.timing_of(&r.action)))
        .filter(|(_, t)| *t != versekit_core::controls::Timing::Follow)
        .collect();
    Bindings { mark, list, names, groups, mouse_only, missing, timings }
}

/// The list with the qualified names added that name a known `actionmap` and a known action (the others stay
/// unknown and are refused like any unknown name).
fn with_qualified(
    mut list: Vec<Resolved>,
    qualified: &[String],
    own: &[(String, Vec<versekit_core::joysticks::Binding>)],
    profile: &versekit_core::controls::Profile,
) -> Vec<Resolved> {
    for name in qualified {
        if game_binding::qualified(name, profile).0.is_some()
            && game_binding::known(name, own, profile)
            && !list.iter().any(|r| &r.action == name)
        {
            list.push(game_binding::resolve(name, own, profile));
        }
    }
    list
}

/// A short mark for the state: the plugin only compares it.
fn short(mark: &str) -> String {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    mark.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

impl Host for DeckHost {
    fn state(&self) -> Map<String, Value> {
        self.refresh_bindings();
        let snapshot = *lock(&self.snapshot);
        let scheme = theme::current();
        let main = crate::app::main_window_of(window_signal::find(Role::Main));
        let mark = lock(&self.bindings).mark.clone();
        let mut map = Map::new();
        map.insert("sprache".into(), json!(language::current().code()));
        map.insert(
            "farben".into(),
            json!({
                "hintergrund": scheme.bg.to_hex(),
                "flaeche": scheme.surface.to_hex(),
                "schrift": scheme.fg.to_hex(),
                "gedaempft": scheme.sub.to_hex(),
                "akzent": scheme.accent.to_hex(),
                "warnung": scheme.yellow.to_hex(),
                "fehler": scheme.red.to_hex(),
                "rahmen": scheme.line.to_hex(),
            }),
        );
        map.insert("overlay".into(), json!(snapshot.overlay_open));
        map.insert("hauptfenster".into(), json!(main.id()));
        map.insert("anti_afk".into(), json!({"an": snapshot.anti_afk}));
        map.insert("mining_scanner".into(), json!({"an": snapshot.mining_scanner}));
        map.insert("durchklick".into(), json!({"an": snapshot.click_through}));
        map.insert("toene".into(), json!({"an": snapshot.sounds}));
        map.insert("raffinerie".into(), self.refinery());
        map.insert("contested".into(), self.contested());
        map.insert("konto".into(), Value::Null);
        // The health comes with the telemetry of Star Citizen 5.0; until VerseKit reads it, there is none.
        map.insert(streamdeck::FIELD_HEALTH.into(), streamdeck::health_value(None));
        map.insert("belegung_stand".into(), json!(if mark.is_empty() { String::new() } else { short(&mark) }));
        map.insert("spielaktion".into(), json!({"gesperrt": self.blocked()}));
        map.insert(streamdeck::FIELD_DEVICE_PAGE.into(), streamdeck::device_page_value(*lock(&self.device_page)));
        map.insert(
            streamdeck::FIELD_DIALS.into(),
            json!({"kompakt": self.paths.setting_bool(streamdeck::SETTING_DIAL_COMPACT, false)}),
        );
        map.insert(streamdeck::FIELD_CONTRACTS.into(), lock(&self.contracts).clone());
        map.insert(streamdeck::FIELD_STATUS.into(), self.readiness(&snapshot).value());
        map.insert(streamdeck::FIELD_SHIP.into(), lock(&self.ship).clone());
        map
    }

    fn command(&self, command: &Command) -> Result<(), Refused> {
        match command {
            Command::Overlay => self.send(|s| &s.overlay),
            Command::RefineryScan => self.send(|s| &s.refinery),
            Command::Picture => self.send(|s| &s.picture),
            Command::Search => self.send(|s| &s.search),
            Command::AntiAfk => self.send(|s| &s.anti_afk),
            Command::MiningScanner => self.send(|s| &s.mining_scanner),
            Command::ClickThrough => self.order(Order::ClickThrough),
            Command::Sounds => self.order(Order::Sounds),
            Command::Blueprint => self.send(|s| &s.blueprint),
            Command::Stock => self.send(|s| &s.stock),
            Command::UnitStock => self.send(|s| &s.unit_stock),
            Command::Main => self.order(Order::Main),
            Command::Page(page) => self.order(Order::Page(page.clone())),
            Command::GameAction { action, phase } => self.game_action(action, *phase),
            Command::GameActionsInUse(names) => {
                *lock(&self.in_use) = names.clone();
                (self.wake)();
                Ok(())
            }
            Command::DeviceHint(hint) => {
                *lock(&self.hint) = hint.clone();
                (self.wake)();
                Ok(())
            }
            Command::PluginVersion(version) => {
                *lock(&self.plugin_version) = Some(version.clone());
                (self.wake)();
                self.offer_newer_plugin(version, Self::shipped_plugin())
            }
            Command::BindingsFolder => match self.bindings_folder() {
                Some(folder) => self.order(Order::Folder(folder)),
                None => Err(Refused { status: 404, code: "kein_ordner" }),
            },
        }
    }

    fn report(&self) -> Map<String, Value> {
        let mut map = Map::new();
        map.insert("geraet_ohne_profil".into(), json!(*lock(&self.hint)));
        map.insert("spielaktionen".into(), json!(*lock(&self.in_use)));
        map.insert("plugin_version".into(), json!(*lock(&self.plugin_version)));
        map
    }

    fn bindings(&self) -> Value {
        self.refresh_bindings();
        let store = lock(&self.bindings);
        let actions: Vec<Value> = store
            .list
            .iter()
            .map(|r| {
                let key = match &r.how {
                    How::Keyboard { input, .. } | How::NotSendable { input } => {
                        versekit_core::controls::input_readable(input, Some(versekit_core::joysticks::Kind::Keyboard))
                    }
                    _ => String::new(),
                };
                json!({
                    "id": r.action,
                    "name": store.names.get(&r.action).cloned().unwrap_or_else(|| r.action.clone()),
                    "gruppe": store.groups.get(&r.group).cloned().unwrap_or_default(),
                    "karte": r.group,
                    // A mouse button that stays: the deck says „Nur Maus“ instead of „Nicht sendbar“.
                    "art": if store.mouse_only.contains(&r.action) { "nur_maus" } else { r.how.word() },
                    "taste": key,
                    "unsicher": matches!(&r.how, How::Keyboard { input, .. } if versekit_core::streamdeck_fill::alt_gr(input)),
                    "quelle": if r.own { "eigene" } else { "standard" },
                    // What the plugin keeps to press the key itself while VerseKit does not run.
                    "eingabe": match &r.how { How::Keyboard { input, .. } => input.clone(), _ => String::new() },
                    "halten": store.timings.get(&r.action).copied().unwrap_or_default().word(),
                })
            })
            .collect();
        json!({"stand": short(&store.mark), "aktionen": actions})
    }
}

/// The running interface.
pub struct Deck {
    pub server: Arc<Server>,
    pub host: Arc<DeckHost>,
    paths: Paths,
    pid: u32,
}

impl Deck {
    /// Starts the interface on a port the system hands out and writes the key file.
    pub fn start(paths: &Paths, host: DeckHost) -> Option<Deck> {
        let listener = streamdeck_server::bind().ok()?;
        let port = listener.local_addr().ok()?.port();
        let key = streamdeck::new_key()?;
        let server = Server::new(key.clone());
        let host = Arc::new(host);
        let pid = std::process::id();
        let own = Paths::new(paths.host.clone());
        streamdeck::write_key_file(&own, &KeyFile { port, key, pid }).ok()?;
        let (s, h): (Arc<Server>, Arc<dyn Host>) = (server.clone(), host.clone());
        std::thread::Builder::new().name("streamdeck".into()).spawn(move || streamdeck_server::serve(listener, s, h)).ok()?;
        let (s, h) = (server.clone(), host.clone());
        std::thread::Builder::new()
            .name("streamdeck-tasten".into())
            .spawn(move || {
                loop {
                    std::thread::sleep(versekit_core::streamdeck_keys::WATCH_EVERY);
                    h.keys.watch(h.sink.as_ref(), s.last_contact(), Instant::now());
                }
            })
            .ok()?;
        Some(Deck { server, host, paths: own, pid })
    }

    /// Switches the interface on (with a new key) or off (key file gone, held keys let go).
    pub fn set_on(&self, on: bool) {
        if on && !self.server.enabled() {
            if let Some(key) = streamdeck::new_key() {
                self.server.set_secret(key.clone());
                let file = KeyFile { port: self.server.port(), key, pid: self.pid };
                let _ = streamdeck::write_key_file(&self.paths, &file);
            }
        } else if !on {
            streamdeck::remove_key_file(&self.paths, self.pid);
            self.host.keys.release_all(self.host.sink.as_ref());
        }
        self.server.set_enabled(on);
    }

    /// The end of the program: keys let go, key file gone.
    pub fn stop(&self) {
        self.set_on(false);
    }

}

/// The host with the system's keys, for the real program.
pub fn system_host(
    host: SystemHost,
    snapshot: Arc<Mutex<Snapshot>>,
    senders: Senders,
    orders: Sender<Order>,
    wake: Arc<dyn Fn() + Send + Sync>,
) -> DeckHost {
    DeckHost::new(Paths::new(host), snapshot, senders, orders, wake, Box::new(SystemKeys))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use versekit_core::anti_afk::{KeyEvent, SendError};

    struct Fake {
        sent: Mutex<Vec<KeyEvent>>,
    }

    struct Shared(Arc<Fake>);

    impl KeySink for Shared {
        fn game_in_front(&self) -> bool {
            true
        }
        fn send(&self, events: &[KeyEvent]) -> Result<(), SendError> {
            self.0.sent.lock().unwrap().extend_from_slice(events);
            Ok(())
        }
    }

    fn host() -> (DeckHost, mpsc::Receiver<()>, mpsc::Receiver<Order>, Arc<Fake>) {
        let (tx, rx) = mpsc::channel();
        let senders = Senders {
            overlay: tx.clone(),
            refinery: tx.clone(),
            picture: tx.clone(),
            search: tx.clone(),
            anti_afk: tx.clone(),
            blueprint: tx.clone(),
            stock: tx.clone(),
            unit_stock: tx.clone(),
            mining_scanner: tx,
        };
        let (orders_tx, orders_rx) = mpsc::channel();
        let fake = Arc::new(Fake { sent: Mutex::new(Vec::new()) });
        // A throwaway data folder of its own: the state reads the refinery jobs and the hangar from there.
        let folder = std::env::temp_dir().join(format!("versekit-deckhost-{}", versekit_core::testdir::run_tag()));
        let _ = std::fs::create_dir_all(&folder);
        let home = folder.to_string_lossy().into_owned();
        let paths = Paths::new(SystemHost::new(
            versekit_core::pypath::Flavor::native(),
            &[(versekit_core::envvars::ENV_HOME, home.as_str())],
            &home,
        ));
        let host = DeckHost::new(paths, Arc::default(), senders, orders_tx, Arc::new(|| {}), Box::new(Shared(fake.clone())));
        (host, rx, orders_rx, fake)
    }

    #[test]
    fn a_hotkey_command_takes_the_channel_of_its_hotkey_and_a_page_becomes_an_order() {
        let (host, presses, orders, _) = host();
        host.command(&Command::Overlay).expect("sent");
        assert!(presses.try_recv().is_ok());
        host.command(&Command::MiningScanner).expect("sent");
        assert!(presses.try_recv().is_ok(), "the mining scanner takes the channel of its hotkey");
        let state = host.state();
        assert_eq!(state["mining_scanner"]["an"], false, "its state goes to the plugin");
        host.command(&Command::Page("hf_bank".into())).expect("sent");
        host.command(&Command::Main).expect("sent");
        assert_eq!(orders.try_iter().collect::<Vec<_>>(), vec![Order::Page("hf_bank".into()), Order::Main]);
    }

    #[test]
    fn click_through_and_sounds_go_to_the_overlay_and_their_state_to_the_plugin() {
        let (host, presses, orders, _) = host();
        host.command(&Command::ClickThrough).expect("sent");
        host.command(&Command::Sounds).expect("sent");
        assert_eq!(orders.try_iter().collect::<Vec<_>>(), vec![Order::ClickThrough, Order::Sounds]);
        assert!(presses.try_recv().is_err(), "no hotkey channel");
        *lock(&host.snapshot) = Snapshot { click_through: true, sounds: false, ..Snapshot::default() };
        let state = host.state();
        assert_eq!((state["durchklick"]["an"].as_bool(), state["toene"]["an"].as_bool()), (Some(true), Some(false)));
    }

    #[test]
    fn an_older_plugin_gets_the_newer_file_opened_once_per_version() {
        let (host, _, orders, _) = host();
        let shipped = || Some(("C:/VerseKit/VerseKit.streamDeckPlugin".to_string(), "5.7.1.200".to_string()));
        // The same or a newer plugin: nothing.
        host.offer_newer_plugin("5.7.1.200", shipped()).expect("same");
        host.offer_newer_plugin("5.7.2.1", shipped()).expect("newer");
        assert!(orders.try_recv().is_err());
        // An older one: the file once.
        host.offer_newer_plugin("5.7.0.100", shipped()).expect("older");
        assert_eq!(orders.try_iter().collect::<Vec<_>>(), vec![Order::PluginFile("C:/VerseKit/VerseKit.streamDeckPlugin".into())]);
        // Declined in the Stream Deck program, the plugin starts again and tells the old version: not again.
        host.offer_newer_plugin("5.7.0.100", shipped()).expect("again");
        assert!(orders.try_recv().is_err(), "once per version VerseKit brings");
        // The next update of VerseKit brings a newer plugin: once more.
        host.offer_newer_plugin("5.7.0.100", Some(("C:/VerseKit/VerseKit.streamDeckPlugin".into(), "5.7.2.300".into())))
            .expect("next");
        assert_eq!(orders.try_iter().count(), 1);
        // Without a plugin file beside VerseKit: nothing.
        host.offer_newer_plugin("5.7.0.100", None).expect("none");
        assert!(orders.try_recv().is_err());
    }

    #[test]
    fn the_version_the_plugin_tells_reaches_the_page() {
        let (host, _, _, _) = host();
        host.command(&Command::PluginVersion("5.7.0.100".into())).expect("told");
        assert_eq!(host.report()["plugin_version"], "5.7.0.100");
    }

    #[test]
    fn the_bindings_folder_is_the_one_of_the_actionmaps_file_or_refused_without_one() {
        let (host, presses, orders, _) = host();
        let wanted = versekit_core::joysticks::actionmaps_path(&host.paths, None)
            .and_then(|file| std::path::Path::new(&file).parent().map(|p| p.to_string_lossy().into_owned()));
        match host.command(&Command::BindingsFolder) {
            Ok(()) => {
                let sent: Vec<Order> = orders.try_iter().collect();
                assert_eq!(sent, vec![Order::Folder(wanted.expect("a game on this machine"))]);
            }
            Err(refused) => {
                assert_eq!((refused.status, refused.code), (404, "kein_ordner"));
                assert!(wanted.is_none(), "a folder is there but was refused");
                assert!(orders.try_recv().is_err(), "nothing opened");
            }
        }
        assert!(presses.try_recv().is_err(), "no hotkey channel");
    }

    #[test]
    fn a_game_action_is_pressed_only_with_its_keyboard_binding() {
        let (host, _, _, fake) = host();
        *lock(&host.bindings) = Bindings {
            mark: "m".into(),
            list: vec![
                Resolved {
                    action: "v_eject".into(),
                    group: "g".into(),
                    how: How::Keyboard {
                        input: "ralt+y".into(),
                        keys: versekit_core::anti_afk_key::parse("ralt+y").expect("keys"),
                    },
                    own: false,
                    deleted: false,
                },
                Resolved { action: "v_mouse".into(), group: "g".into(), how: How::MouseOnly, own: false, deleted: false },
            ],
            ..Bindings::default()
        };
        if host.blocked().is_some() {
            // A test run with administrator rights cannot check the pressing; the refusal is the right answer then.
            let refused = host.command(&Command::GameAction { action: "v_eject".into(), phase: Phase::Down });
            assert!(refused.is_err());
            return;
        }
        host.command(&Command::GameAction { action: "v_eject".into(), phase: Phase::Down }).expect("down");
        host.command(&Command::GameAction { action: "v_eject".into(), phase: Phase::Up }).expect("up");
        assert_eq!(fake.sent.lock().unwrap().len(), 4);
        let mouse = host.command(&Command::GameAction { action: "v_mouse".into(), phase: Phase::Down });
        assert_eq!(mouse, Err(Refused { status: 409, code: "nicht_sendbar" }));
        let unknown = host.command(&Command::GameAction { action: "v_nix".into(), phase: Phase::Down });
        assert_eq!(unknown, Err(Refused { status: 400, code: "aktion" }));
        assert_eq!(fake.sent.lock().unwrap().len(), 4, "nothing more sent");
    }

    #[test]
    fn a_tap_on_the_deck_holds_the_key_as_long_as_the_game_wants_it_held() {
        let (host, _, _, fake) = host();
        let keys = versekit_core::anti_afk_key::parse("lalt+lshift+b").expect("keys");
        let profile = versekit_core::controls::Profile::from_xml(
            r#"<ActionMaps><ActivationMode name="delayed_press" onPress="1" multiTap="1" pressTriggerThreshold="0.25" releaseTriggerThreshold="-1"/>
               <actionmap name="spaceship_movement"><action name="v_master_mode_cycle_long" activationMode="delayed_press"/></actionmap></ActionMaps>"#,
        )
        .expect("profile");
        *lock(&host.bindings) = Bindings {
            mark: "m".into(),
            list: vec![Resolved {
                action: "v_master_mode_cycle_long".into(),
                group: "spaceship_movement".into(),
                how: How::Keyboard { input: "lalt+lshift+b".into(), keys },
                own: true,
                deleted: false,
            }],
            timings: [("v_master_mode_cycle_long".to_string(), profile.timing_of("v_master_mode_cycle_long"))].into(),
            ..Bindings::default()
        };
        if host.blocked().is_some() {
            return;
        }
        let start = Instant::now();
        host.command(&Command::GameAction { action: "v_master_mode_cycle_long".into(), phase: Phase::Down }).expect("down");
        host.command(&Command::GameAction { action: "v_master_mode_cycle_long".into(), phase: Phase::Up }).expect("up");
        let ups = || fake.sent.lock().unwrap().iter().filter(|e| e.up).count();
        assert_eq!((host.keys.count(), ups()), (1, 0), "a tap of the deck does not let go at once");
        host.keys.watch(host.sink.as_ref(), Some(Duration::ZERO), start + Duration::from_millis(200));
        assert_eq!(ups(), 0, "not before the game took it");
        host.keys.watch(host.sink.as_ref(), Some(Duration::ZERO), start + Duration::from_millis(400));
        assert_eq!((host.keys.count(), ups()), (0, 3), "let go after the threshold");
    }

    #[test]
    fn a_qualified_name_on_a_key_is_allowed_only_for_a_known_actionmap_and_action() {
        let profile = versekit_core::controls::Profile::from_xml(
            r#"<ActionMaps>
              <actionmap name="spaceship_view" UILabel="@v"><action name="v_view_zoom_in" UILabel="@z"/></actionmap>
              <actionmap name="vehicle_general" UILabel="@g"><action name="v_view_zoom_in" UILabel="@z"/></actionmap>
            </ActionMaps>"#,
        )
        .expect("profile");
        let own = versekit_core::joysticks::bindings_from_text(
            r#"<ActionMaps version="1"><ActionProfiles version="1" profileName="default">
              <actionmap name="vehicle_general"><action name="v_view_zoom_in"><rebind input="kb1_ralt+8"/></action></actionmap>
            </ActionProfiles></ActionMaps>"#,
        );
        let names: Vec<String> =
            ["vehicle_general.v_view_zoom_in", "gibt_es_nicht.v_view_zoom_in", "vehicle_general.v_nix"].iter().map(|s| s.to_string()).collect();
        let list = with_qualified(game_binding::resolve_all(&own, &profile), &names, &own, &profile);
        let added: Vec<&Resolved> = list.iter().filter(|r| r.action.contains('.')).collect();
        assert_eq!(added.len(), 1, "{added:?}");
        assert!(matches!(&added[0].how, How::Keyboard { input, .. } if input == "ralt+8"));
        // The host presses it like any other action of the list; an unknown qualified name is refused.
        let (host, _, _, _) = host();
        *lock(&host.bindings) = Bindings { mark: "m".into(), list, ..Bindings::default() };
        if host.blocked().is_none() {
            let unknown = host.command(&Command::GameAction { action: "gibt_es_nicht.v_view_zoom_in".into(), phase: Phase::Down });
            assert_eq!(unknown, Err(Refused { status: 400, code: "aktion" }));
            host.command(&Command::GameAction { action: "vehicle_general.v_view_zoom_in".into(), phase: Phase::Down }).expect("down");
            host.command(&Command::GameAction { action: "vehicle_general.v_view_zoom_in".into(), phase: Phase::Up }).expect("up");
        }
    }

    #[test]
    fn every_wish_for_a_page_of_the_device_gets_a_number_of_its_own() {
        let (host, _, _, _) = host();
        assert!(host.state()[streamdeck::FIELD_DEVICE_PAGE].is_null(), "no wish before the first");
        host.ask_page(streamdeck::DevicePage::OnFoot);
        assert_eq!(host.state()[streamdeck::FIELD_DEVICE_PAGE], json!({"nr": 1, "seite": "zu_fuss"}));
        assert_eq!(host.state()[streamdeck::FIELD_DEVICE_PAGE]["nr"], 1, "asking the state again asks for nothing new");
        host.ask_page(streamdeck::DevicePage::OnFoot);
        assert_eq!(host.state()[streamdeck::FIELD_DEVICE_PAGE]["nr"], 2, "the same page once more is a new wish");
    }

    #[test]
    fn the_readiness_says_the_game_the_log_and_how_many_keys_of_the_deck_have_no_key() {
        let (host, _, _, _) = host();
        assert_eq!(host.readiness(&Snapshot::default()).keys_missing, 0, "the bindings not worked out yet: none counted");
        // The bindings are known: of three actions on the deck one has a key, one only the mouse (missing, as the rule
        // of the bar says), one is unknown (no missing key: the bar does not count it either).
        // The mark of now: the state does not work the bindings out anew over the ones of the test.
        *lock(&host.in_use) = vec!["v_eject".into(), "v_mouse".into(), "v_nix".into()];
        *lock(&host.bindings) = Bindings {
            mark: host.wanted_mark(),
            list: vec![
                Resolved {
                    action: "v_eject".into(),
                    group: "g".into(),
                    how: How::Keyboard { input: "ralt+y".into(), keys: versekit_core::anti_afk_key::parse("ralt+y").expect("keys") },
                    own: false,
                    deleted: false,
                },
                Resolved { action: "v_mouse".into(), group: "g".into(), how: How::MouseOnly, own: false, deleted: false },
            ],
            missing: ["v_mouse".to_string()].into_iter().collect(),
            ..Bindings::default()
        };
        *lock(&host.snapshot) = Snapshot { game_runs: true, log_found: Some(true), ..Snapshot::default() };
        assert_eq!(host.state()[streamdeck::FIELD_STATUS], json!({"spiel_laeuft": true, "log_ok": true, "tasten_fehlen": 1}));
        *lock(&host.snapshot) = Snapshot { game_runs: false, log_found: None, ..Snapshot::default() };
        assert_eq!(host.state()[streamdeck::FIELD_STATUS], json!({"spiel_laeuft": false, "log_ok": null, "tasten_fehlen": 1}));
        assert_eq!(host.state()[streamdeck::FIELD_SHIP], Value::Null, "no ship before the log named one");
        host.set_ship(Some("Aegis Sabre"));
        assert_eq!(host.state()[streamdeck::FIELD_SHIP], json!({"name": "Aegis Sabre"}));
        host.set_ship(None);
        assert_eq!(host.state()[streamdeck::FIELD_SHIP], Value::Null, "the game ended: the ship goes");
    }

    #[test]
    fn the_list_of_the_bindings_names_the_actionmap_of_each_action() {
        let (host, _, _, _) = host();
        *lock(&host.bindings) = Bindings {
            mark: host.wanted_mark(),
            list: vec![Resolved { action: "v_power_toggle_shields".into(), group: "spaceship_power".into(), how: How::MouseOnly, own: false, deleted: false }],
            ..Bindings::default()
        };
        let list = host.bindings();
        let entry = &list["aktionen"][0];
        assert_eq!(entry["id"], "v_power_toggle_shields");
        assert_eq!(entry["karte"], "spaceship_power", "the plugin chooses symbol and colour by it");
        assert_eq!(entry["art"], "nur_maus");
        assert_eq!(entry["unsicher"], false);
    }

    #[test]
    fn a_keyboard_binding_with_alt_gr_is_named_unsafe_and_one_without_is_not() {
        let (host, _, _, _) = host();
        let keyboard = |input: &str| How::Keyboard { input: input.into(), keys: Vec::new() };
        *lock(&host.bindings) = Bindings {
            mark: host.wanted_mark(),
            list: vec![
                Resolved { action: "v_a".into(), group: "g".into(), how: keyboard("ralt+b"), own: false, deleted: false },
                Resolved { action: "v_b".into(), group: "g".into(), how: keyboard("lalt+lshift+b"), own: true, deleted: false },
            ],
            ..Bindings::default()
        };
        let list = host.bindings();
        assert_eq!((list["aktionen"][0]["unsicher"].clone(), list["aktionen"][1]["unsicher"].clone()), (json!(true), json!(false)));
    }

    #[test]
    fn a_mouse_button_that_stays_says_mouse_only_and_counts_as_no_missing_key() {
        let (host, _, _, _) = host();
        *lock(&host.in_use) = vec!["v_master".into(), "v_moveable".into()];
        let not_sendable = || How::NotSendable { input: "mouse4".into() };
        *lock(&host.bindings) = Bindings {
            mark: host.wanted_mark(),
            list: vec![
                Resolved { action: "v_master".into(), group: "g".into(), how: not_sendable(), own: true, deleted: false },
                Resolved { action: "v_moveable".into(), group: "g".into(), how: not_sendable(), own: true, deleted: false },
            ],
            mouse_only: ["v_master".to_string()].into_iter().collect(),
            missing: ["v_moveable".to_string()].into_iter().collect(),
            ..Bindings::default()
        };
        assert_eq!(host.readiness(&Snapshot::default()).keys_missing, 1, "only the one that can still move is missing");
        let list = host.bindings();
        assert_eq!(list["aktionen"][0]["art"], "nur_maus", "stays on the mouse");
        assert_eq!(list["aktionen"][1]["art"], "nicht_sendbar", "moves on the next putting in");
    }

    #[test]
    fn the_state_has_every_field_of_the_contract() {
        let (host, _, _, _) = host();
        let state = host.state();
        for field in streamdeck::STATE_FIELDS {
            if matches!(*field, "version" | "rev") {
                continue;
            }
            assert!(state.contains_key(*field), "{field}");
        }
    }
}
