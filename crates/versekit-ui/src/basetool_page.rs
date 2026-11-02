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

//! The page "Settings, Connection & sync" (`basetool_page.py`): connect to
//! the KRT Profit Basetool with a code, one switch and one line of state per
//! area, the sync button with what the last pass did, and the cards that ask
//! the player when the two sides differ.
//!
//! ⚠⚠ **Only the code and the bare address are shown, copied and opened**,
//! never a link with the code built in: the page the player types the code
//! on warns about fraud.
//!
//! ⚠ **The page keeps its state.** The sync publishes its status and the login
//! as values with a generation; the page draws what is there each frame, and
//! what the player typed (the name of the connection) stays.
//!
//! ⚠ Every area has one statement: the code is shown above, a permission is
//! missing, or the state of the last pass. An area the program does not build
//! yet keeps its switch off and says so.

use std::sync::{Arc, OnceLock};

use egui::{Align, Key, RichText, Ui, vec2};
use versekit_core::basetool::blueprint_area::{self, BlueprintArea, BlueprintHost, FileBlueprintHost};
use versekit_core::basetool::connection::{Connection, system_clock};
use versekit_core::basetool::decisions;
use versekit_core::basetool::demand::{DemandArea, DemandStore};
use versekit_core::basetool::demo::{DemoBasetool, DemoSync};
use versekit_core::basetool::error::error_text;
use versekit_core::basetool::http::HttpTransport;
use versekit_core::basetool::key::system_keys;
use versekit_core::basetool::orgstock::{OrgStock, OrgStockArea};
use versekit_core::basetool::orgunits::{OrgUnit, OrgUnitsArea, label as unit_label};
use versekit_core::basetool::overview::{self, AreaCard, CARDS, DecisionCard, DecisionKind, DemandInfo, Verdict};
use versekit_core::basetool::scopes::{
    SCOPES_DEMAND_DETAILS, SCOPES_ORGSTOCK_DETAILS, SCOPES_ORGSTOCK_WRITE, SCOPES_ORGUNITS, SETTING_DEMAND,
    SETTING_LABEL, SETTING_NEW_SHIP_UNIT, SETTING_ORGSTOCK, SETTING_ORGSTOCK_WRITE, SETTING_SHIPS, SETTING_STOCK,
    all_in, enabled,
};
use versekit_core::basetool::secrets::{Backend, SystemSecrets};
use versekit_core::basetool::ship_area::ShipArea;
use versekit_core::basetool::stock_area::StockArea;
use versekit_core::basetool::sync::{Login, Phase, Setup, Shared, Status, SyncHandle};
use versekit_core::basetool::{config, hold};
use versekit_core::jsonio::{Map, Value};
use versekit_core::language::{Arg, t, tf};
use versekit_core::paths::Paths;
use versekit_core::theme::{Color, Scheme};

use crate::layout::GAP;
use crate::widgets::{self, ButtonKind};
use crate::{color, icons};

/// How long "copied" shows on a copy button.
const COPIED_SECONDS: f64 = 2.0;

/// What the pages that talk to the Basetool hold: the sync of the program
/// and the stores it fills.
#[derive(Clone)]
pub struct Wire {
    pub sync: SyncHandle,
    pub org_stock: Arc<OrgStock>,
    /// What the open orders of the units still need, in memory only.
    pub demand: Arc<DemandStore>,
    pub units: Shared<Vec<OrgUnit>>,
    /// Set by the first page that draws; the sync wakes the window through it.
    pub repaint: Arc<OnceLock<egui::Context>>,
    /// The own game account, to pick the own entry as owner.
    pub own_account: Option<String>,
    /// The made-up Basetool of the preview: no browser opens, the page says so.
    pub demo: Option<Arc<DemoBasetool>>,
    /// The folder the credentials sit in, if the system has no keyring.
    pub file_store: Option<String>,
    /// Puts blueprints (by name) back into the collection when the player
    /// takes back what was removed here; set where the blueprint sync exists.
    pub restore: Option<Arc<dyn Fn(Vec<String>) + Send + Sync>>,
}

impl Wire {
    /// A sync against the made-up Basetool with its settings in a throwaway
    /// folder. `worker` starts the sync in the background.
    pub fn preview(worker: bool) -> Wire {
        // One folder per preview: windows of one process do not share settings.
        static COUNT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let number = COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let folder = std::env::temp_dir().join(format!("versekit-vorschau-{}-{number}", std::process::id()));
        // A folder an earlier process with the same number left behind holds nothing for this one.
        let _ = std::fs::remove_dir_all(&folder);
        Wire::in_folder(&folder.to_string_lossy(), worker)
    }

    /// Like [`Wire::preview`], with the settings in `folder`.
    pub fn in_folder(folder: &str, worker: bool) -> Wire {
        let _ = std::fs::create_dir_all(folder);
        let repaint: Arc<OnceLock<egui::Context>> = Arc::new(OnceLock::new());
        let waker = {
            let repaint = repaint.clone();
            Arc::new(move || {
                if let Some(ctx) = repaint.get() {
                    ctx.request_repaint();
                }
            })
        };
        let own = Some("PlayerOne".to_string());
        let world = DemoSync::new(folder, waker, own.clone(), worker);
        Wire {
            sync: world.sync,
            org_stock: world.org_stock,
            demand: world.demand,
            units: world.units,
            repaint,
            own_account: own,
            demo: Some(world.demo),
            file_store: None,
            restore: None,
        }
    }

    /// The sync of the program against the real Basetool: its settings, the
    /// system's store for token and key, the real address (or the test
    /// environment of the environment variables). Nothing goes out before a
    /// switch is on and someone connects. `None` if the connection cannot be
    /// made (the reason is in the error log).
    pub fn system(
        paths: Arc<Paths>,
        own_account: versekit_core::basetool::sync::OwnAccount,
        version: &str,
    ) -> Option<Wire> {
        let host = paths.host.clone();
        let config = config::Config::from_host(&host);
        let transport = match HttpTransport::new(&config.ca, &host) {
            Ok(transport) => Arc::new(transport),
            Err(error) => {
                versekit_core::errors::record("basetool_page.transport", &error);
                return None;
            }
        };
        let secrets = Arc::new(SystemSecrets::from_host(&host));
        let keys = system_keys(secrets.clone(), &host);
        let file_store = (secrets.backend() == Backend::File).then(|| secrets.folder().to_string());
        let clock = system_clock();
        // A copy of the data (`--daten`) holds the credentials of the real installation: nothing of it reaches
        // the Basetool unless a test opens it (`testmode::ENV_BASETOOL_IN_TEST`).
        let locked = versekit_core::testmode::basetool_locked(&paths);
        // A test against the real Basetool sets a ceiling for the changes of the whole run
        // (`testmode::ENV_BASETOOL_WRITE_MAX`, one budget however often the connection is made anew) and asks
        // for the request journal; without the variables nothing changes. A copy that signs in with the key of
        // the real installation never deletes it.
        let budget =
            versekit_core::testmode::basetool_write_ceiling(&paths).map(versekit_core::basetool::guard::run_budget);
        let conn = Arc::new(
            Connection::new(config, transport, secrets, keys, version, clock.clone())
                .with_test_lock(locked)
                .with_kept_key(versekit_core::testmode::basetool_keeps_key(&paths))
                .with_write_budget(budget)
                .with_journal(versekit_core::basetool::guard::Journal::from_env(&paths)),
        );
        let repaint: Arc<OnceLock<egui::Context>> = Arc::new(OnceLock::new());
        let waker = {
            let repaint = repaint.clone();
            Arc::new(move || {
                if let Some(ctx) = repaint.get() {
                    ctx.request_repaint();
                }
            })
        };
        let org_stock = OrgStock::new();
        let demand = DemandStore::new();
        let units = OrgUnitsArea::default();
        let unit_list = units.units.clone();
        let account = own_account();
        // The collection of the blueprints: the sync writes it as the window does, and the watcher of the overlay
        // takes the file over before it writes its next find.
        let catalog = Arc::new(ListCatalog { paths: paths.clone() });
        let blueprints = Arc::new(FileBlueprintHost::new(paths.clone(), catalog));
        let restore: Arc<dyn Fn(Vec<String>) + Send + Sync> = {
            let host = blueprints.clone();
            Arc::new(move |names| {
                let changes: Vec<blueprint_area::Change> =
                    names.into_iter().map(|name| blueprint_area::Change::Add { name, time: None }).collect();
                host.apply(&changes);
            })
        };
        let mut ships = ShipArea::new(unit_list.clone());
        ships.on_names = Some(crate::ship_names::names_hook(paths.host.clone()));
        let sync = SyncHandle::start(Setup {
            conn,
            paths,
            clock,
            version: version.to_string(),
            own_account,
            waker,
            // The order of 4.12: blueprints, stock, the list of units, the hangar, the demand, then the unit
            // storage (an error of the demand stops the pass, so the storage after it waits).
            areas: vec![
                Box::new(BlueprintArea::new(blueprints)),
                Box::new(StockArea),
                Box::new(units),
                Box::new(ships),
                Box::new(DemandArea { store: demand.clone() }),
                Box::new(OrgStockArea { store: org_stock.clone() }),
            ],
        });
        Some(Wire {
            sync,
            org_stock,
            demand,
            units: unit_list,
            repaint,
            own_account: account,
            demo: None,
            file_store,
            restore: Some(restore),
        })
    }

    /// A sync with the areas of the program against the stand-in of the
    /// Basetool in memory (`ExchangeDouble`), the settings and the data in
    /// `folder`; no worker, a pass runs when the test says so. For the tests.
    pub fn over_double(double: &Arc<versekit_core::basetool::exchange_double::ExchangeDouble>, folder: &str) -> Wire {
        let _ = std::fs::create_dir_all(folder);
        let host = versekit_core::paths::Host::new(
            versekit_core::pypath::Flavor::native(),
            &[(versekit_core::legacy::ENV_HOME, folder)],
            folder,
        );
        let paths = Arc::new(Paths::new(host));
        let units = OrgUnitsArea::default();
        let unit_list = units.units.clone();
        let org_stock = OrgStock::new();
        let demand = DemandStore::new();
        let catalog = Arc::new(versekit_core::collection::NoCatalog);
        let blueprints = Arc::new(FileBlueprintHost::new(paths.clone(), catalog));
        let clock = system_clock();
        let sync = SyncHandle::without_worker(Setup {
            conn: Arc::new(double.connection(clock.clone())),
            paths,
            clock,
            version: "5.0.0".into(),
            own_account: Arc::new(|| Some("PlayerOne".to_string())),
            waker: Arc::new(|| {}),
            areas: vec![
                Box::new(BlueprintArea::new(blueprints)),
                Box::new(StockArea),
                Box::new(units),
                Box::new(ShipArea::new(unit_list.clone())),
                Box::new(DemandArea { store: demand.clone() }),
                Box::new(OrgStockArea { store: org_stock.clone() }),
            ],
        });
        Wire {
            sync,
            org_stock,
            demand,
            units: unit_list,
            repaint: Arc::new(OnceLock::new()),
            own_account: Some("PlayerOne".into()),
            demo: None,
            file_store: None,
            restore: None,
        }
    }

    /// Tells the sync which window to wake once something changed; every
    /// frame of a page that uses the sync does it.
    pub fn remember(&self, ctx: &egui::Context) {
        let _ = self.repaint.get_or_init(|| ctx.clone());
    }
}

/// The catalog the collection matches the names of the Basetool against: the blueprints of the list, as the
/// overlay reads them (offline, nothing is fetched here).
struct ListCatalog {
    paths: Arc<Paths>,
}

impl versekit_core::collection::Catalog for ListCatalog {
    fn known(&self) -> Result<versekit_core::collection::Known, String> {
        let catalog = versekit_gamedata::list::list_catalog_offline(&self.paths);
        Ok(versekit_core::collection::Known::new(
            catalog.iter().flat_map(|c| c.blueprints.iter()).map(|b| (b.key.clone(), Some(b.name.clone()))),
        ))
    }
}

/// What the line under the connection says after something happened.
#[derive(Clone, Debug, PartialEq)]
enum Said {
    Disconnected,
}

/// A notice at the top right: text and when it came.
type Notice = (String, f64);

/// Which copy button was pressed last, to show "copied" on it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Copied {
    Code,
    Address,
}

/// The page.
pub struct ConnectionState {
    wire: Option<Wire>,
    said: Option<Said>,
    /// A click on "Sync now" waits for the pass: the generation of the status
    /// when it was pressed.
    requested: Option<u64>,
    /// The name of the connection as typed.
    label: String,
    label_error: bool,
    /// The name field had the focus in the frame before.
    label_focus: bool,
    /// The id of the name field, known after its first frame.
    label_id: Option<egui::Id>,
    copied: Option<(Copied, f64)>,
    notice: Option<Notice>,
    disconnecting: Option<versekit_core::basetool::sync::Job<()>>,
    /// The sync state of the installation, read again when the status changes
    /// or the player decided.
    state: Option<(u64, Map)>,
    state_stale: bool,
    /// The generation of the login whose code was scrolled to.
    scrolled_to: u64,
}

impl Drop for ConnectionState {
    /// The sync in the background ends with the window; closing it does not
    /// wait for a request that is on its way.
    fn drop(&mut self) {
        if let Some(wire) = &self.wire {
            wire.sync.stop();
        }
    }
}

impl Default for ConnectionState {
    fn default() -> Self {
        ConnectionState::new(None)
    }
}

/// What the player did this frame; applied after drawing.
enum Act {
    Connect,
    Cancel,
    Disconnect,
    Copy(Copied, String),
    Browser(String),
    Switch(&'static str, bool),
    SyncNow,
    ConfirmAccount,
    Decide(DecisionKind, bool, Vec<String>),
    NewShipUnit(String),
    Label,
}

impl ConnectionState {
    pub fn new(wire: Option<Wire>) -> ConnectionState {
        ConnectionState {
            label: wire.as_ref().map(|w| w.sync.label()).unwrap_or_default(),
            wire,
            said: None,
            requested: None,
            label_error: false,
            label_focus: false,
            label_id: None,
            copied: None,
            notice: None,
            disconnecting: None,
            state: None,
            state_stale: true,
            scrolled_to: 0,
        }
    }

    pub fn wire(&self) -> Option<&Wire> {
        self.wire.as_ref()
    }

    fn tell(&mut self, text: String, now: f64) {
        self.notice = Some((text, now));
    }

    fn open_browser(&mut self, url: &str, now: f64) {
        if self.wire.as_ref().is_some_and(|w| w.demo.is_some()) {
            self.tell(t("s_sle_beispiel_browser"), now);
        } else {
            versekit_core::browser::open_in_browser(url);
        }
    }

    /// Commits the typed name of the connection (focus lost, Enter).
    fn keep_label(&mut self, wire: &Wire) {
        let mut value = self.label.trim().to_string();
        if value.is_empty() {
            value = versekit_core::testmode::basetool_default_label(wire.sync.paths()).to_string();
            self.label = value.clone();
        }
        if !config::label_valid(&value) {
            self.label_error = true;
            return;
        }
        self.label_error = false;
        if value != wire.sync.label() {
            wire.sync.set_setting(SETTING_LABEL, Value::str(&value));
            wire.sync.request_now();
        }
    }

    fn apply(&mut self, act: Act, ctx: &egui::Context, now: f64) {
        let Some(wire) = self.wire.clone() else { return };
        let sync = &wire.sync;
        match act {
            Act::Connect => {
                self.said = None;
                sync.connect();
            }
            Act::Cancel => sync.cancel_login(),
            Act::Disconnect => {
                if self.disconnecting.is_none() {
                    self.disconnecting = Some(sync.disconnect());
                }
            }
            Act::Copy(which, text) => {
                ctx.copy_text(text);
                self.copied = Some((which, now));
                self.tell(t("s_btv_kopiert"), now);
            }
            Act::Browser(url) => self.open_browser(&url, now),
            Act::Switch(setting, on) => {
                sync.set_setting(setting, Value::Bool(on));
                sync.switched();
            }
            Act::SyncNow => {
                self.requested = Some(sync.status().generation());
                sync.release_held(&[]);
                self.tell(t("s_btv_laeuft"), now);
            }
            Act::ConfirmAccount => {
                sync.confirm_account();
            }
            Act::Decide(kind, first, keys) => self.decide(&wire, kind, first, keys, now),
            Act::NewShipUnit(id) => {
                sync.set_setting(SETTING_NEW_SHIP_UNIT, Value::str(id));
            }
            Act::Label => self.keep_label(&wire),
        }
    }

    /// A decision of the player: written into the state, a pass follows.
    /// `first` is the first button of the card.
    fn decide(&mut self, wire: &Wire, kind: DecisionKind, first: bool, keys: Vec<String>, now: f64) {
        let sync = &wire.sync;
        let done = match (kind, first) {
            (DecisionKind::BlueprintsGone, _) => sync.decide(decisions::override_conflicts(keys)),
            (DecisionKind::BlueprintsRemovedHere, true) => sync.decide(decisions::release_removals(keys)),
            (DecisionKind::BlueprintsRemovedHere, false) => {
                let Some(state) = sync.state() else { return };
                // Taking them back needs the collection: without the way
                // to it the blueprints would leave the list and be lost.
                let Some(restore) = wire.restore.clone() else { return };
                let (change, names) = decisions::keep_removed(&state, &keys);
                // First back into the collection, then the decision: it wakes a pass, which must not meet the
                // blueprints as removed once more.
                restore(names);
                sync.decide(change)
            }
            (DecisionKind::StockAmounts, mine) => {
                sync.decide(decisions::stock_decide(keys, if mine { "mine" } else { "theirs" }))
            }
            (DecisionKind::ShipsMissing, true) => sync.decide(decisions::ships_release(keys)),
            (DecisionKind::ShipsMissing, false) => sync.decide(decisions::ships_keep(keys)),
            (DecisionKind::ShipsGone, _) => sync.decide(decisions::ships_override(keys)),
        };
        if done {
            self.state_stale = true;
            self.tell(t("s_btv_vorgemerkt"), now);
        }
    }
}

// ---------------------------------------------------------------- drawing

fn small(ui: &mut Ui, text: &str, tone: Color) {
    ui.add(egui::Label::new(RichText::new(text).small().color(color(tone))).wrap());
}

fn button<'a>(text: &'a str, kind: ButtonKind, scheme: &'a Scheme) -> widgets::Button<'a> {
    widgets::Button::new(text, kind, scheme)
}

/// What the page needs from the world this frame.
struct Frame {
    connected: bool,
    granted: Vec<String>,
    status: Status,
    status_generation: u64,
    login: Option<Arc<Login>>,
    login_generation: u64,
    switches: [bool; 5],
    write_on: bool,
    any_on: bool,
    now: f64,
    units: Vec<OrgUnit>,
    /// The test mode: the connection lets nothing through.
    locked: bool,
}

impl Frame {
    fn login_active(&self) -> bool {
        matches!(self.login.as_deref(), Some(Login::Starting | Login::Code { .. }))
    }
}

fn frame_of(wire: &Wire) -> Frame {
    let sync = &wire.sync;
    let paths = sync.paths();
    let (status_generation, status) = sync.status().get();
    let (login_generation, login) = sync.login_state().get();
    let switches = CARDS.map(|card| paths.setting_bool(card.setting, false));
    Frame {
        connected: sync.linked(),
        granted: sync.granted(),
        status: status.map(|s| (*s).clone()).unwrap_or_default(),
        status_generation,
        login,
        login_generation,
        switches,
        write_on: paths.setting_bool(SETTING_ORGSTOCK_WRITE, false),
        any_on: enabled(paths.as_ref()),
        now: sync.now(),
        units: wire.units.get().1.map(|u| (*u).clone()).unwrap_or_default(),
        locked: sync.connection().test_locked(),
    }
}

/// The page.
pub fn show_page(ui: &mut Ui, scheme: &Scheme, page: &mut ConnectionState) {
    let ctx = ui.ctx().clone();
    let now = ui.input(|i| i.time);
    let Some(wire) = page.wire.clone() else { return };
    wire.remember(&ctx);
    small(ui, &t("s_btv_lead"), scheme.sub);
    if wire.demo.is_some() {
        ui.horizontal_wrapped(|row| {
            if let Some(image) = icons::themed("info", scheme, versekit_core::theme::IconTint::Highlight, 14.0) {
                row.add(image);
            }
            small(row, &t("s_btv_beispiel"), scheme.sub);
        });
    }
    ui.add_space(GAP);

    // A job whose thread broke off lets go, too: "Disconnect" must not stay blocked.
    if page.disconnecting.as_mut().is_some_and(|job| job.poll().is_some() || job.lost()) {
        page.disconnecting = None;
        page.said = Some(Said::Disconnected);
        page.requested = None;
        page.state_stale = true;
    }
    let frame = frame_of(&wire);
    // The hint "syncing" ends when a pass that began after the click is over.
    if page.requested.is_some_and(|asked| frame.status_generation > asked) && frame.status.phase != Phase::Running {
        page.requested = None;
    }
    if page.state.as_ref().map(|(g, _)| *g) != Some(frame.status_generation) {
        page.state_stale = true;
    }
    if page.state_stale {
        page.state = wire.sync.state().map(|s| (frame.status_generation, s));
        page.state_stale = false;
    }
    // The state is taken out for the frame instead of copied: it can hold
    // thousands of blueprints.
    let state_held = page.state.take();
    let no_state = Map::new();
    let state: &Map = state_held.as_ref().map_or(&no_state, |(_, s)| s);

    let mut acts: Vec<Act> = Vec::new();
    let top = ui.cursor().min;
    connection_card(ui, scheme, page, &wire, &frame, &mut acts);
    ui.add_space(GAP);
    // The areas stand in two columns when there is room, one below the other
    // in the order of the list when not.
    if ui.available_width() >= 2.0 * crate::settings::SETTINGS_COLUMN_MIN + GAP {
        crate::layout::columns_or_stack(ui, crate::settings::SETTINGS_COLUMN_MIN, 2, |ui, column| {
            for card in CARDS.iter().skip(column).step_by(2) {
                area_card(ui, scheme, &wire, &frame, state, card, &mut acts);
                ui.add_space(GAP);
            }
        });
    } else {
        for card in &CARDS {
            area_card(ui, scheme, &wire, &frame, state, card, &mut acts);
            ui.add_space(GAP);
        }
    }
    if frame.connected && frame.any_on && !frame.login_active() {
        sync_card(ui, scheme, page, &frame, &mut acts);
        ui.add_space(GAP);
    }
    if frame.status.code.starts_with("account_") {
        account_card(ui, scheme, &frame, &mut acts);
        ui.add_space(GAP);
    }
    if frame.connected && !state.is_empty() {
        for card in overview::decision_cards(state) {
            decision_card(ui, scheme, &wire, &card, &mut acts);
            ui.add_space(GAP);
        }
    }
    small(ui, &t("s_btv_datenschutz"), scheme.sub);
    ui.add_space(GAP);

    // A new code is scrolled to: it stands at the top.
    if matches!(frame.login.as_deref(), Some(Login::Code { .. })) && page.scrolled_to != frame.login_generation {
        page.scrolled_to = frame.login_generation;
        ui.scroll_to_rect(egui::Rect::from_min_size(top, vec2(1.0, 1.0)), Some(Align::TOP));
    }
    page.state = state_held;
    for act in acts {
        page.apply(act, &ctx, now);
    }
    if let Some((text, since)) = page.notice.clone()
        && !widgets::notice(&ctx, scheme, since, "check", &text, None).shown
    {
        page.notice = None;
    }
    if frame.status.phase == Phase::Running || page.disconnecting.is_some() {
        ctx.request_repaint_after(std::time::Duration::from_millis(300));
    }
}

/// The yellow line of the write guard (`VERSEKIT_BASETOOL_WRITE_MAX`): what this run may send at most and what
/// is left of it; `None` while there is no guard.
fn guard_text(max: Option<u32>, left: Option<u32>) -> Option<String> {
    match max? {
        0 => Some(t("s_btv_schreibwache_holen")),
        n => Some(tf(
            "s_btv_schreibwache_lauf",
            &[("n", Arg::from(n as usize)), ("rest", Arg::from(left.unwrap_or(0) as usize))],
        )),
    }
}

/// The card "Connection": warnings, the state, the name, the buttons or the
/// code to type.
fn connection_card(
    ui: &mut Ui,
    scheme: &Scheme,
    page: &mut ConnectionState,
    wire: &Wire,
    f: &Frame,
    acts: &mut Vec<Act>,
) {
    widgets::card(ui, scheme, |ui| {
        ui.set_width(ui.available_width());
        ui.label(RichText::new(t("s_btv_verbindung")).strong());
        let config = wire.sync.connection().config();
        if config.sandbox {
            small(ui, &t("s_btv_sandbox"), scheme.yellow);
        } else if config.overridden {
            small(ui, &t("s_btv_override"), scheme.yellow);
        }
        let conn = wire.sync.connection();
        if conn.test_locked() {
            small(ui, &t("s_btv_testbetrieb"), scheme.yellow);
        } else if conn.key_kept() {
            small(ui, &t("s_btv_schluessel_bleibt"), scheme.yellow);
        }
        if let Some(text) = guard_text(conn.write_guard(), conn.write_left()) {
            small(ui, &text, scheme.yellow);
        }
        if let Some(folder) = &wire.file_store {
            small(ui, &tf("s_btv_datei", &[("ordner", Arg::from(folder.as_str()))]), scheme.yellow);
        }
        ui.add_space(4.0);
        match f.login.as_deref() {
            Some(Login::Starting) => {
                small(ui, &t("s_btv_hole_code"), scheme.sub);
            }
            Some(Login::Code { user_code, verification_uri }) => {
                login_box(ui, scheme, page, user_code, verification_uri, acts);
            }
            _ if f.connected => {
                let head = tf("s_btv_verbunden", &[("name", Arg::from(wire.sync.label()))]);
                widgets::status_box(ui, scheme, scheme.accent, "circle-check", &head, "");
            }
            _ => {
                widgets::status_box(ui, scheme, scheme.sub, "circle-dashed", &t("s_btv_nicht_verbunden"), "");
            }
        }
        if let Some(text) = message(page, f) {
            small(ui, &text, scheme.yellow);
        }
        ui.add_space(6.0);
        label_row(ui, scheme, page, wire, acts);
        if !f.login_active() {
            ui.add_space(6.0);
            if f.connected {
                ui.horizontal_wrapped(|row| {
                    let busy = page.disconnecting.is_some();
                    let word = t("s_btv_trennen");
                    if row.add_enabled(!busy, button(&word, ButtonKind::Danger, scheme)).clicked() {
                        acts.push(Act::Disconnect);
                    }
                });
            } else if !f.any_on {
                small(ui, &t("s_btv_erst_bereich"), scheme.yellow);
            } else {
                let word = t("s_btv_verbinden");
                if ui.add(button(&word, ButtonKind::Strong, scheme).icon("arrow-left-right")).clicked() {
                    acts.push(Act::Connect);
                }
            }
        }
    });
}

/// The line the last login or disconnecting left (yellow).
fn message(page: &ConnectionState, f: &Frame) -> Option<String> {
    if let Some(Said::Disconnected) = page.said {
        return Some(t("s_btv_getrennt"));
    }
    // The Basetool ended the connection (revoked, run out): the reason stays
    // on the page, where it stood only as "not connected" before. It gives
    // way to a login that is running or has just ended with its own result.
    // The lock of the test mode has its own line above the state box.
    let said_above = |code: &str| f.locked && code == versekit_core::testmode::BASETOOL_LOCKED;
    let ended = (!f.connected && f.status.phase == Phase::Error && !f.status.code.is_empty())
        .then_some(f.status.code.as_str())
        .filter(|code| !said_above(code))
        .map(error_text);
    match f.login.as_deref() {
        Some(Login::Starting | Login::Code { .. }) => None,
        Some(Login::Denied) => Some(t("s_btv_abgelehnt")),
        Some(Login::Expired) => Some(t("s_btv_abgelaufen")),
        Some(Login::Failed(code)) => (!said_above(code)).then(|| error_text(code)),
        // Connected: the state box above says it.
        Some(Login::Connected | Login::Cancelled) | None => ended,
    }
}

/// Whether the field takes the saved name again: not while it is being typed
/// in, not in the frame in which it lost the focus (its text is saved then),
/// and not while its error stands.
fn reload_label(has_focus: bool, had_focus: bool, error: bool) -> bool {
    !has_focus && !had_focus && !error
}

/// The name of this installation in the Basetool, with its error and hint.
fn label_row(ui: &mut Ui, scheme: &Scheme, page: &mut ConnectionState, wire: &Wire, acts: &mut Vec<Act>) {
    let has_focus = page.label_id.is_some_and(|id| ui.memory(|m| m.has_focus(id)));
    if reload_label(has_focus, page.label_focus, page.label_error) {
        page.label = wire.sync.label();
    }
    ui.label(RichText::new(t("s_btv_name")).small().color(color(scheme.sub)));
    let hint = t("s_btv_name_beispiel");
    let error = page.label_error.then(|| t("s_btv_name_falsch"));
    let width = ui.available_width().min(260.0);
    let response = widgets::checked_field(ui, scheme, &mut page.label, &hint, width, error.as_deref());
    page.label_id = Some(response.id);
    let (shown, name) = (page.label.clone(), t("s_btv_name"));
    response.widget_info(|| {
        let mut info = egui::WidgetInfo::text_edit(true, "", &shown, &hint);
        info.label = Some(name.clone());
        info
    });
    let enter = response.has_focus() && ui.input(|i| i.key_pressed(Key::Enter));
    page.label_focus = response.has_focus();
    if response.lost_focus() || enter {
        acts.push(Act::Label);
    }
    small(ui, &t("s_btv_name_h"), scheme.sub);
}

/// The code to type and the bare address; copy and browser buttons.
fn login_box(ui: &mut Ui, scheme: &Scheme, page: &mut ConnectionState, code: &str, address: &str, acts: &mut Vec<Act>) {
    small(ui, &t("s_btv_code_h"), scheme.sub);
    ui.add_space(4.0);
    let now = ui.input(|i| i.time);
    let copied = |which: Copied, page: &ConnectionState| {
        page.copied.is_some_and(|(c, at)| c == which && now - at < COPIED_SECONDS)
    };
    ui.label(RichText::new(code).heading().color(color(scheme.accent)));
    ui.add_space(4.0);
    ui.label(RichText::new(t("s_btv_adresse_h")).small());
    if wrapping_link(ui, scheme, address).clicked() {
        acts.push(Act::Browser(address.to_string()));
    }
    ui.add_space(4.0);
    small(ui, &t("s_btv_code_warnung"), scheme.yellow);
    ui.add_space(6.0);
    // One row of buttons: the browser first, both copies, then cancelling.
    ui.horizontal_wrapped(|row| {
        let open = t("s_btv_browser");
        if row.add(button(&open, ButtonKind::Strong, scheme).icon("external-link")).clicked() {
            acts.push(Act::Browser(address.to_string()));
        }
        for (which, key, text) in
            [(Copied::Code, "s_btv_code_kopieren", code), (Copied::Address, "s_btv_adresse_kopieren", address)]
        {
            let word = t(key);
            let icon = if copied(which, page) { "check" } else { "copy" };
            if row.add(button(&word, ButtonKind::Normal, scheme).icon(icon)).clicked() {
                acts.push(Act::Copy(which, text.to_string()));
            }
        }
        let cancel = t("s_bt_abbrechen");
        if row.add(button(&cancel, ButtonKind::Normal, scheme)).clicked() {
            acts.push(Act::Cancel);
        }
    });
}

/// An address in the accent that wraps anywhere and opens on a click,
/// underlined while the mouse is over it.
fn wrapping_link(ui: &mut Ui, scheme: &Scheme, address: &str) -> egui::Response {
    let font = egui::TextStyle::Body.resolve(ui.style());
    let icon_room = 18.0;
    let width = (ui.available_width() - icon_room).max(40.0);
    let mut job = egui::text::LayoutJob::simple(address.to_string(), font, color(scheme.accent), width);
    job.wrap.break_anywhere = true;
    let galley = ui.fonts_mut(|fonts| fonts.layout_job(job));
    let size = vec2(galley.size().x + icon_room, galley.size().y);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    if ui.is_rect_visible(rect) {
        let icon_at = egui::Rect::from_min_size(rect.min + vec2(0.0, 2.0), vec2(14.0, 14.0));
        if let Some(image) = icons::themed("external-link", scheme, versekit_core::theme::IconTint::Highlight, 14.0) {
            image.paint_at(ui, icon_at);
        }
        let text_at = rect.min + vec2(icon_room, 0.0);
        ui.painter().galley(text_at, galley.clone(), color(scheme.accent));
        if response.hovered() {
            let y = rect.bottom() - 1.0;
            ui.painter().hline(text_at.x..=rect.right(), y, egui::Stroke::new(1.0, color(scheme.accent)));
        }
    }
    widgets::focus_ring(ui, &response, scheme, crate::control_radius(scheme));
    let text = address.to_string();
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Link, true, &text));
    response
}

/// The rights of an area that are not granted.
fn missing_of(scopes: &[&str], granted: &[String]) -> Vec<String> {
    scopes.iter().filter(|s| !granted.iter().any(|g| g == *s)).map(|s| (*s).to_string()).collect()
}

/// Whether a new login can grant any of `missing`: the Basetool must offer it.
fn askable(wire: &Wire, missing: &[String]) -> bool {
    let refused = wire.sync.connection().not_offered(missing);
    missing.iter().any(|s| !refused.contains(s))
}

/// A permission that is missing: the hint and the button for a new login, or
/// the hint that the Basetool does not offer it yet. Returns whether it
/// showed anything.
fn consent(
    ui: &mut Ui,
    scheme: &Scheme,
    wire: &Wire,
    f: &Frame,
    scopes: &[&str],
    text_key: &str,
    acts: &mut Vec<Act>,
) -> bool {
    if f.granted.is_empty() || all_in(scopes, &f.granted) {
        return false;
    }
    let missing = missing_of(scopes, &f.granted);
    if !askable(wire, &missing) {
        small(ui, &t("s_btv_nicht_angeboten"), scheme.sub);
        return true;
    }
    small(ui, &t(text_key), scheme.yellow);
    let word = t("s_btv_rechte_erweitern");
    if ui.add(button(&word, ButtonKind::Strong, scheme)).clicked() {
        acts.push(Act::Connect);
    }
    true
}

/// One area: the switch, what it does, and its one statement.
fn area_card(ui: &mut Ui, scheme: &Scheme, wire: &Wire, f: &Frame, state: &Map, card: &AreaCard, acts: &mut Vec<Act>) {
    let index = CARDS.iter().position(|c| c.setting == card.setting).unwrap_or(0);
    let on = f.switches[index];
    let built = wire.sync.has_area(area_name(card.setting));
    widgets::card(ui, scheme, |ui| {
        ui.set_width(ui.available_width());
        let title = t(card.title);
        let mut flip = on;
        crate::settings::setting_row(ui, scheme, &title, &t(card.help), false, |row| {
            let switch = row.add_enabled(built || on, widgets::Toggle::new(&mut flip, scheme));
            let (enabled, now) = (switch.enabled(), flip);
            switch.widget_info(|| egui::WidgetInfo::selected(egui::Role::CheckBox, enabled, now, &title));
        });
        if flip != on {
            acts.push(Act::Switch(card.setting, flip));
        }
        if !built {
            small(ui, &t("s_btv_noch_nicht"), scheme.sub);
            return;
        }
        if !(on && f.connected) {
            return;
        }
        if f.login_active() {
            small(ui, &t("s_btv_code_oben"), scheme.yellow);
            return;
        }
        if !f.granted.is_empty() && !all_in(card.scopes, &f.granted) {
            consent(ui, scheme, wire, f, card.scopes, "s_btv_rechte_fehlen", acts);
            return;
        }
        // The demand lives in memory only: the line says what it holds, or that the right is missing.
        let demand: Option<DemandInfo> = (card.setting == SETTING_DEMAND)
            .then(|| {
                if !wire.demand.reason().is_empty() {
                    return Some(DemandInfo { no_right: true, materials: 0, items: 0, orders: None });
                }
                wire.demand.current(f.now).1.map(|data| DemandInfo {
                    no_right: false,
                    materials: data.materials.len(),
                    items: data.items.len(),
                    orders: data.orders.as_ref().map(Vec::len),
                })
            })
            .flatten();
        for line in overview::area_lines(card.setting, &f.status.counts, state, demand.as_ref()) {
            small(ui, &line, scheme.fg);
        }
        held_note(ui, scheme, wire, state, card.setting, f.now, acts);
        match card.setting {
            SETTING_SHIPS => {
                consent(ui, scheme, wire, f, SCOPES_ORGUNITS, "s_btv_einheit_recht", acts);
                new_ship_unit(ui, scheme, wire, f, acts);
            }
            SETTING_STOCK => {
                consent(ui, scheme, wire, f, SCOPES_ORGUNITS, "s_btv_einheit_recht_lager", acts);
            }
            SETTING_DEMAND => {
                consent(ui, scheme, wire, f, SCOPES_DEMAND_DETAILS, "s_btv_rechte_zusatz", acts);
            }
            SETTING_ORGSTOCK => {
                write_row(ui, scheme, f, acts);
                let mut wanted: Vec<&str> = SCOPES_ORGSTOCK_DETAILS.to_vec();
                if f.write_on {
                    wanted.extend_from_slice(SCOPES_ORGSTOCK_WRITE);
                }
                consent(ui, scheme, wire, f, &wanted, "s_btv_rechte_zusatz", acts);
            }
            _ => {}
        }
    });
}

/// The name the sync gives an area (`Area::name`).
fn area_name(setting: &str) -> &'static str {
    match setting {
        SETTING_ORGSTOCK => overview::COUNTS_ORGSTOCK,
        SETTING_STOCK => overview::COUNTS_STOCK,
        SETTING_SHIPS => overview::COUNTS_SHIPS,
        SETTING_DEMAND => overview::COUNTS_DEMAND,
        _ => overview::COUNTS_BLUEPRINTS,
    }
}

/// A change set the Basetool holds back: the hint and, while the link is
/// valid, the button to the confirmation.
fn held_note(ui: &mut Ui, scheme: &Scheme, wire: &Wire, state: &Map, setting: &str, now: f64, acts: &mut Vec<Act>) {
    let Some(route) = overview::route_of(setting) else { return };
    let Some((_, how)) = hold::held_routes(state, now).into_iter().find(|(r, _)| r == route) else { return };
    if how == "expired" {
        small(ui, &t("s_btv_gesperrt_alt"), scheme.yellow);
        return;
    }
    let link = wire.sync.confirm_link(route);
    small(ui, &t(if link.is_some() { "s_btv_masse_h" } else { "s_btv_gesperrt" }), scheme.yellow);
    if let Some(url) = link {
        let word = t("s_btv_masse_oeffnen");
        if ui.add(button(&word, ButtonKind::Strong, scheme).icon("external-link")).clicked() {
            acts.push(Act::Browser(url));
        }
    }
}

/// "Book as a logistician": only with reading, off from the start.
fn write_row(ui: &mut Ui, scheme: &Scheme, f: &Frame, acts: &mut Vec<Act>) {
    ui.add_space(4.0);
    let title = t("s_btv_staffellager_buchen");
    let mut flip = f.write_on;
    crate::settings::setting_row(ui, scheme, &title, &t("s_btv_staffellager_buchen_h"), false, |row| {
        let switch = row.add(widgets::Toggle::new(&mut flip, scheme));
        let (enabled, now) = (switch.enabled(), flip);
        switch.widget_info(|| egui::WidgetInfo::selected(egui::Role::CheckBox, enabled, now, &title));
    });
    if flip != f.write_on {
        acts.push(Act::Switch(SETTING_ORGSTOCK_WRITE, flip));
    }
}

/// The unit for new ships: only with more than one unit; a unit the member
/// left goes back to "as before".
fn new_ship_unit(ui: &mut Ui, scheme: &Scheme, wire: &Wire, f: &Frame, acts: &mut Vec<Act>) {
    if f.units.len() < 2 {
        return;
    }
    let mut entries: Vec<(String, String)> = vec![(String::new(), t("s_btv_einheit_wie_bisher"))];
    entries.extend(f.units.iter().map(|u| (u.id.clone(), unit_label(u))));
    let mut current = wire.sync.paths().setting(SETTING_NEW_SHIP_UNIT).unwrap_or_default();
    if !entries.iter().any(|(id, _)| *id == current) {
        // Left unit: back to "as before", or it would apply again after a return.
        if !current.is_empty() {
            acts.push(Act::NewShipUnit(String::new()));
        }
        current.clear();
    }
    let shown = entries.iter().find(|(id, _)| *id == current).map(|(_, l)| l.clone()).unwrap_or_default();
    let name = t("s_btv_einheit_neu");
    ui.add_space(4.0);
    let mut chosen = current.clone();
    crate::settings::setting_row(ui, scheme, &name, &t("s_btv_einheit_neu_h"), false, |row| {
        crate::settings::choice_box(row, "basetool-einheit-neu", &name, &shown, |list| {
            for (id, label) in &entries {
                list.selectable_value(&mut chosen, id.clone(), label.as_str());
            }
        });
    });
    if chosen != current {
        acts.push(Act::NewShipUnit(chosen));
    }
}

/// The card "Sync": the time of the last pass or that one runs, what stopped
/// it, and the button.
fn sync_card(ui: &mut Ui, scheme: &Scheme, page: &mut ConnectionState, f: &Frame, acts: &mut Vec<Act>) {
    widgets::card(ui, scheme, |ui| {
        ui.set_width(ui.available_width());
        ui.label(RichText::new(t("s_btv_abgleich")).strong());
        let running = f.status.phase == Phase::Running || page.requested.is_some();
        if running {
            widgets::loading(ui, scheme, &t("s_btv_laeuft"));
        } else if f.status.last_sync.is_some() {
            ui.add(
                egui::Label::new(tf("s_btv_zuletzt", &[("zeit", Arg::from(overview::last_sync_text(&f.status)))]))
                    .wrap(),
            );
        }
        let reason = t("s_btv_testbetrieb");
        if f.locked {
            small(ui, &reason, scheme.yellow);
        } else if !f.status.code.is_empty() && f.status.phase != Phase::Running {
            small(ui, &error_text(&f.status.code), scheme.yellow);
        }
        ui.add_space(6.0);
        let word = t("s_btv_jetzt");
        let sync =
            button(&word, ButtonKind::Normal, scheme).icon("rotate-cw").blocked(f.locked.then_some(reason.as_str()));
        if ui.add(sync).clicked() {
            acts.push(Act::SyncNow);
        }
    });
}

/// The card "Which account?" when the check stopped the sync.
fn account_card(ui: &mut Ui, scheme: &Scheme, f: &Frame, acts: &mut Vec<Act>) {
    widgets::card(ui, scheme, |ui| {
        ui.set_width(ui.available_width());
        ui.label(RichText::new(t("s_btv_konto")).strong());
        let tone = if f.status.code == "account_mismatch" { scheme.red } else { scheme.yellow };
        small(ui, &error_text(&f.status.code), tone);
        if f.status.code == "account_unknown" {
            ui.add_space(6.0);
            let word = t("s_btv_konto_bestaetigen");
            if ui.add(button(&word, ButtonKind::Strong, scheme)).clicked() {
                acts.push(Act::ConfirmAccount);
            }
        }
    });
}

/// A card that asks the player: what it is about and the buttons.
fn decision_card(ui: &mut Ui, scheme: &Scheme, wire: &Wire, card: &DecisionCard, acts: &mut Vec<Act>) {
    // The areas that settle the answer are not all built: the buttons wait.
    let area = match card.kind {
        DecisionKind::BlueprintsGone | DecisionKind::BlueprintsRemovedHere => overview::COUNTS_BLUEPRINTS,
        DecisionKind::StockAmounts => overview::COUNTS_STOCK,
        DecisionKind::ShipsMissing | DecisionKind::ShipsGone => overview::COUNTS_SHIPS,
    };
    let waiting = t("s_btv_noch_nicht");
    let reason = (!wire.sync.has_area(area)).then_some(waiting.as_str());
    widgets::card(ui, scheme, |ui| {
        ui.set_width(ui.available_width());
        ui.label(RichText::new(&card.title).strong());
        small(ui, &card.help, scheme.sub);
        ui.add_space(4.0);
        ui.add(egui::Label::new(&card.listing).wrap());
        ui.add_space(6.0);
        let (first, second): ((String, ButtonKind), Option<(String, ButtonKind)>) = match card.kind {
            DecisionKind::BlueprintsGone | DecisionKind::ShipsGone => {
                ((t("s_btv_wieder_hoch"), ButtonKind::Normal), None)
            }
            DecisionKind::BlueprintsRemovedHere => {
                ((t("s_btv_auch_dort"), ButtonKind::Danger), Some((t("s_btv_behalten"), ButtonKind::Normal)))
            }
            DecisionKind::StockAmounts => {
                ((t("s_btv_meine"), ButtonKind::Normal), Some((t("s_btv_deren"), ButtonKind::Normal)))
            }
            DecisionKind::ShipsMissing => {
                ((t("s_btv_auch_dort"), ButtonKind::Danger), Some((t("s_btv_hangar_behalten"), ButtonKind::Normal)))
            }
        };
        ui.horizontal_wrapped(|row| {
            if row.add(button(&first.0, first.1, scheme).blocked(reason)).clicked() {
                acts.push(Act::Decide(card.kind, true, card.keys.clone()));
            }
            // Taking blueprints back also needs the way into the collection.
            let reason_second = reason.or_else(|| {
                (card.kind == DecisionKind::BlueprintsRemovedHere && wire.restore.is_none()).then_some(waiting.as_str())
            });
            if let Some((text, kind)) = &second
                && row.add(button(text, *kind, scheme).blocked(reason_second)).clicked()
            {
                acts.push(Act::Decide(card.kind, false, card.keys.clone()));
            }
        });
    });
}

/// The result line of a sync button on another page, once a pass is over:
/// `(text, tone)`; `None` while it runs.
pub fn result_line(wire: &Wire, setting: Option<&str>) -> Option<(String, Verdict)> {
    let status = wire.sync.status().get().1?;
    if status.phase == Phase::Running {
        return None;
    }
    let state = wire.sync.state().unwrap_or_default();
    Some(overview::sync_result(&status, &state, setting, wire.sync.now()))
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use versekit_core::basetool::demo::DemoLogin;
    use versekit_core::jsonio::map_set;

    use super::*;

    fn folder(name: &str) -> String {
        let dir =
            std::env::temp_dir().join(format!("versekit-verbindung-{name}-{}", versekit_core::testdir::run_tag()));
        let _ = std::fs::remove_dir_all(&dir);
        dir.to_string_lossy().into_owned()
    }

    /// A page on a made-up Basetool that answers at once; the sync has no
    /// worker, a pass runs when the test says so.
    fn page(name: &str) -> (ConnectionState, String) {
        let dir = folder(name);
        let wire = Wire::in_folder(&dir, false);
        wire.demo.as_ref().expect("demo").script_login(DemoLogin::Approve, 1);
        (ConnectionState::new(Some(wire)), dir)
    }

    fn wire_of(page: &ConnectionState) -> Wire {
        page.wire().cloned().expect("wire")
    }

    fn wait_for(seconds: u64, mut done: impl FnMut() -> bool) -> bool {
        let end = Instant::now() + Duration::from_secs(seconds);
        while Instant::now() < end {
            if done() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        done()
    }

    /// Every text of a frame, as drawn, lower case.
    fn drawn(page: &mut ConnectionState, width: f32) -> (f32, Vec<String>) {
        fn collect(shape: &egui::Shape, out: &mut Vec<String>) {
            match shape {
                egui::Shape::Text(text) => out.push(text.galley.text().to_lowercase()),
                egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| collect(s, out)),
                _ => {}
            }
        }
        let scheme = versekit_core::theme::current();
        let ctx = egui::Context::default();
        crate::install(&ctx);
        let (mut overflow, mut texts) = (0.0, Vec::new());
        for _ in 0..3 {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, 3000.0))),
                ..Default::default()
            };
            texts.clear();
            let mut output = ctx.run_ui(input, |ui| {
                overflow = crate::layout::page(ui, scheme, "seite", |p| show_page(p, scheme, page)).overflow;
            });
            output.shapes.iter().for_each(|clipped| collect(&clipped.shape, &mut texts));
            output.textures_delta.clear();
        }
        (overflow, texts)
    }

    /// Whether the frame shows a text, in either language.
    fn shows(texts: &[String], key: &str) -> bool {
        use versekit_core::language::{Lang, t_in};
        [Lang::De, Lang::En].iter().any(|lang| {
            let wanted = t_in(*lang, key).to_lowercase();
            texts.iter().any(|shown| shown.contains(&wanted))
        })
    }

    /// Whether the frame shows a text as a whole (a button, a title), in
    /// either language; `shows` also finds it inside a longer sentence.
    fn shows_alone(texts: &[String], key: &str) -> bool {
        use versekit_core::language::{Lang, t_in};
        [Lang::De, Lang::En].iter().any(|lang| texts.contains(&t_in(*lang, key).to_lowercase()))
    }

    fn connect(page: &mut ConnectionState) {
        let ctx = egui::Context::default();
        page.apply(Act::Connect, &ctx, 0.0);
        let wire = wire_of(page);
        assert!(
            wait_for(20, || wire.sync.login_state().get().1.as_deref() == Some(&Login::Connected)),
            "the code was not granted"
        );
    }

    /// Before anything is switched on there is nothing to connect with: the
    /// hint stands where the button will be.
    #[test]
    fn an_area_comes_first_then_the_connect_button() {
        let (mut page, dir) = page("erst");
        let (_, texts) = drawn(&mut page, 900.0);
        assert!(shows(&texts, "s_btv_erst_bereich"));
        assert!(shows(&texts, "s_btv_nicht_verbunden"));
        assert!(!shows_alone(&texts, "s_btv_jetzt") && !shows_alone(&texts, "s_btv_trennen"));
        assert!(!shows_alone(&texts, "s_btv_verbinden"), "no button without an area");
        let ctx = egui::Context::default();
        page.apply(Act::Switch(SETTING_ORGSTOCK, true), &ctx, 0.0);
        let wire = wire_of(&page);
        assert!(wire.sync.paths().setting_bool(SETTING_ORGSTOCK, false), "the switch is in the settings");
        let (_, texts) = drawn(&mut page, 900.0);
        assert!(!shows(&texts, "s_btv_erst_bereich"));
        assert!(shows_alone(&texts, "s_btv_verbinden"));
        let _ = std::fs::remove_dir_all(dir);
    }

    /// The way through: the code is shown with the bare address, then the
    /// state says connected, the sync card appears, and the last pass is read
    /// off the area.
    #[test]
    fn a_login_shows_the_code_then_the_state_of_the_pass() {
        let (mut page, dir) = page("code");
        let wire = wire_of(&page);
        let ctx = egui::Context::default();
        page.apply(Act::Switch(SETTING_ORGSTOCK, true), &ctx, 0.0);
        page.apply(Act::Connect, &ctx, 0.0);
        assert!(wait_for(10, || matches!(wire.sync.login_state().get().1.as_deref(), Some(Login::Code { .. }))));
        let (overflow, texts) = drawn(&mut page, 900.0);
        assert_eq!(overflow, 0.0);
        assert!(texts.iter().any(|t| t == &versekit_core::basetool::demo::USER_CODE.to_lowercase()), "{texts:?}");
        assert!(shows(&texts, "s_btv_code_warnung") && shows_alone(&texts, "s_btv_browser"));
        assert!(
            !shows_alone(&texts, "s_btv_verbinden") && !shows_alone(&texts, "s_btv_trennen"),
            "no button while the code stands"
        );
        assert!(wait_for(20, || wire.sync.login_state().get().1.as_deref() == Some(&Login::Connected)));
        wire.sync.pass_now().expect("pass");
        let (overflow, texts) = drawn(&mut page, 900.0);
        assert_eq!(overflow, 0.0);
        let state_box = tf("s_btv_verbunden", &[("name", Arg::from(wire.sync.label()))]).to_lowercase();
        assert!(texts.contains(&state_box), "the state box names the connection: {state_box}");
        assert!(shows_alone(&texts, "s_btv_trennen") && shows_alone(&texts, "s_btv_jetzt"));
        assert!(!shows_alone(&texts, "s_btv_browser"), "the code box is gone once connected");
        let status = wire.sync.status().get().1.expect("status");
        let last = tf("s_btv_zuletzt", &[("zeit", Arg::from(overview::last_sync_text(&status)))]).to_lowercase();
        assert!(texts.contains(&last), "the time of the last pass: {last}");
        let counts = tf("s_btv_staffellager_stand", &[("einheiten", Arg::from(3)), ("posten", Arg::from(9))]);
        assert!(texts.iter().any(|t| t.contains(&counts.to_lowercase())), "{counts}");
        let _ = std::fs::remove_dir_all(dir);
    }

    /// Cancelling the code leaves a page that can connect again; a refused
    /// code says so.
    #[test]
    fn a_cancelled_or_refused_code_is_said_and_nothing_is_connected() {
        let (mut page, dir) = page("abbruch");
        let wire = wire_of(&page);
        let ctx = egui::Context::default();
        page.apply(Act::Switch(SETTING_ORGSTOCK, true), &ctx, 0.0);
        page.apply(Act::Connect, &ctx, 0.0);
        assert!(wait_for(10, || matches!(wire.sync.login_state().get().1.as_deref(), Some(Login::Code { .. }))));
        page.apply(Act::Cancel, &ctx, 0.0);
        let (_, texts) = drawn(&mut page, 900.0);
        assert!(!shows(&texts, "s_btv_browser"), "the code box is gone");
        assert!(!wire.sync.connection().connected());
        wire.demo.as_ref().expect("demo").script_login(DemoLogin::Deny, 1);
        page.apply(Act::Connect, &ctx, 0.0);
        assert!(wait_for(20, || wire.sync.login_state().get().1.as_deref() == Some(&Login::Denied)));
        let (_, texts) = drawn(&mut page, 900.0);
        assert!(shows(&texts, "s_btv_abgelehnt"));
        let _ = std::fs::remove_dir_all(dir);
    }

    /// Areas the program does not build yet keep their switch off and say so;
    /// the unit storage is built.
    #[test]
    fn areas_that_are_not_built_say_so() {
        let (mut page, dir) = page("nichtda");
        let wire = wire_of(&page);
        let (_, texts) = drawn(&mut page, 900.0);
        let missing = CARDS.iter().filter(|c| !wire.sync.has_area(area_name(c.setting))).count();
        assert!(missing >= 1 && missing < CARDS.len(), "some built, some not: {missing}");
        let said = texts.iter().filter(|t| shows(std::slice::from_ref(t), "s_btv_noch_nicht")).count();
        assert_eq!(said, missing);
        assert!(wire.sync.has_area(overview::COUNTS_ORGSTOCK));
        let _ = std::fs::remove_dir_all(dir);
    }

    /// The name: a wrong one is refused and stays unsaved, an empty one gives
    /// the default, a good one is saved and asks for a sync.
    #[test]
    fn the_name_of_the_connection_follows_the_rules() {
        let (mut page, dir) = page("name");
        let wire = wire_of(&page);
        let before = wire.sync.label();
        page.label = "kaputt/\\".into();
        page.keep_label(&wire);
        assert!(page.label_error);
        assert_eq!(wire.sync.label(), before);
        // While the error stands, the page does not overwrite the typed text.
        let (_, texts) = drawn(&mut page, 900.0);
        assert!(shows(&texts, "s_btv_name_falsch"));
        assert_eq!(page.label, "kaputt/\\");
        page.label = "Mein Rechner 2".into();
        page.keep_label(&wire);
        assert!(!page.label_error);
        assert_eq!(wire.sync.label(), "Mein Rechner 2");
        page.label = "   ".into();
        page.keep_label(&wire);
        assert_eq!(page.label, config::default_label());
        assert_eq!(wire.sync.label(), config::default_label());
        let _ = std::fs::remove_dir_all(dir);
    }

    /// The decisions land in the state file of the installation and a sync
    /// follows; the buttons of an area that is not built wait.
    #[test]
    fn a_decision_is_written_into_the_state_and_asks_for_a_sync() {
        let (mut page, dir) = page("entscheidung");
        let wire = wire_of(&page);
        let ctx = egui::Context::default();
        page.apply(Act::Switch(SETTING_ORGSTOCK, true), &ctx, 0.0);
        connect(&mut page);
        wire.sync.pass_now().expect("pass");
        let seed: versekit_core::basetool::sync::Change = Arc::new(|state: &mut Map| {
            map_set(state, "local_removed", Value::Object(vec![("k1".into(), Value::str("bt1"))]));
            map_set(
                state,
                "baseline",
                Value::Object(vec![(
                    "bt1".into(),
                    Value::Object(vec![(
                        "ref".into(),
                        Value::Object(vec![("name".into(), Value::str("Pembroke Helm"))]),
                    )]),
                )]),
            );
        });
        assert!(wire.sync.decide(seed));
        page.state_stale = true;
        let (overflow, texts) = drawn(&mut page, 700.0);
        assert_eq!(overflow, 0.0);
        assert!(texts.iter().any(|t| t.contains("pembroke helm")), "{texts:?}");
        assert!(shows(&texts, "s_btv_auch_dort") && shows(&texts, "s_btv_behalten"));
        page.apply(Act::Decide(DecisionKind::BlueprintsRemovedHere, true, vec!["k1".into()]), &ctx, 1.0);
        let state = wire.sync.state().expect("state");
        let pending = state.iter().find(|(k, _)| k == "pending_removals").map(|(_, v)| v.clone());
        assert_eq!(pending, Some(Value::Array(vec![Value::str("k1")])));
        assert!(page.notice.is_some(), "the note that it is written down");
        let _ = std::fs::remove_dir_all(dir);
    }

    /// The stop of the account check gets its own card; only the unknown
    /// account has a button to confirm.
    #[test]
    fn the_account_card_shows_its_reason() {
        let (mut page, dir) = page("konto");
        let wire = wire_of(&page);
        let ctx = egui::Context::default();
        page.apply(Act::Switch(SETTING_ORGSTOCK, true), &ctx, 0.0);
        connect(&mut page);
        wire.demo.as_ref().expect("demo").script_account("unknown");
        wire.sync.pass_now().expect("pass");
        let (_, texts) = drawn(&mut page, 900.0);
        assert!(shows(&texts, "s_btv_konto") && shows(&texts, "s_btv_konto_bestaetigen"));
        page.apply(Act::ConfirmAccount, &ctx, 0.0);
        wire.sync.pass_now().expect("pass");
        let (_, texts) = drawn(&mut page, 900.0);
        assert!(!shows(&texts, "s_btv_konto_bestaetigen"));
        let _ = std::fs::remove_dir_all(dir);
    }

    /// A wrong account only says so: the button to confirm belongs to the
    /// unknown account alone.
    #[test]
    fn a_wrong_account_has_no_button_to_confirm() {
        let (mut page, dir) = page("konto-falsch");
        let wire = wire_of(&page);
        let ctx = egui::Context::default();
        wire.demo.as_ref().expect("demo").script_account("mismatch");
        page.apply(Act::Switch(SETTING_ORGSTOCK, true), &ctx, 0.0);
        connect(&mut page);
        wire.sync.pass_now().expect("pass");
        assert!(wire.sync.status().get().1.is_some_and(|s| s.code == "account_mismatch"));
        let (_, texts) = drawn(&mut page, 900.0);
        assert!(shows(&texts, "s_btv_konto") && shows(&texts, "s_btv_konto_falsch"), "{texts:?}");
        assert!(!shows(&texts, "s_btv_konto_bestaetigen"));
        let _ = std::fs::remove_dir_all(dir);
    }

    /// The switch of an area the program does not build yet cannot be flipped,
    /// not even with the keyboard; the switch of a built one can.
    #[test]
    fn the_switch_of_an_area_that_is_not_built_stays_off() {
        let (page, dir) = page("schalter");
        let wire = wire_of(&page);
        let (frame, state) = (frame_of(&wire), wire.sync.state().unwrap_or_default());
        let mut flipped = Vec::new();
        for card in CARDS.iter() {
            let built = wire.sync.has_area(area_name(card.setting));
            for tabs in 1..=3 {
                let acts =
                    keyed(900.0, tabs, |ui, scheme, acts| area_card(ui, scheme, &wire, &frame, &state, card, acts));
                if acts.iter().any(|a| matches!(a, Act::Switch(..))) {
                    flipped.push(built);
                }
            }
        }
        assert!(flipped.iter().any(|built| *built), "the switch of a built area flips by keyboard");
        assert!(flipped.iter().all(|built| *built), "the switch of an area that is not built flipped");
        let _ = std::fs::remove_dir_all(dir);
    }

    /// The buttons of a decision wait for the areas that are not built, also
    /// for the keyboard.
    #[test]
    fn the_buttons_of_a_decision_wait_for_an_area_that_is_not_built() {
        let (page, dir) = page("wartet");
        let wire = wire_of(&page);
        let free = keyed(900.0, 1, |ui, scheme, acts| {
            if ui.add(button("frei", ButtonKind::Normal, scheme)).clicked() {
                acts.push(Act::Cancel);
            }
        });
        assert!(!free.is_empty(), "a button that is not blocked answers the keyboard");
        for kind in [
            DecisionKind::BlueprintsGone,
            DecisionKind::BlueprintsRemovedHere,
            DecisionKind::StockAmounts,
            DecisionKind::ShipsMissing,
            DecisionKind::ShipsGone,
        ] {
            let card = DecisionCard {
                kind,
                title: "Titel".into(),
                help: "Hilfe".into(),
                listing: "Liste".into(),
                keys: vec!["k1".into()],
            };
            for tabs in 1..=3 {
                let acts = keyed(900.0, tabs, |ui, scheme, acts| decision_card(ui, scheme, &wire, &card, acts));
                assert!(acts.is_empty(), "{kind:?} answered while its area is not built");
            }
        }
        let _ = std::fs::remove_dir_all(dir);
    }

    /// The hint that a pass runs stays from the click until a pass that began
    /// after it is over, also when an earlier pass already left its time.
    #[test]
    fn the_sync_hint_stays_until_the_pass_is_over() {
        let (mut page, dir) = page("laeuft");
        let wire = wire_of(&page);
        let ctx = egui::Context::default();
        page.apply(Act::Switch(SETTING_ORGSTOCK, true), &ctx, 0.0);
        connect(&mut page);
        wire.sync.pass_now().expect("pass");
        let (_, texts) = drawn(&mut page, 900.0);
        assert!(page.requested.is_none());
        assert!(shows_alone(&texts, "s_btv_jetzt"));
        page.apply(Act::SyncNow, &ctx, 1.0);
        assert!(page.requested.is_some());
        // Several frames without a new pass: the hint stays, though a time is there.
        let (_, texts) = drawn(&mut page, 900.0);
        assert!(shows_alone(&texts, "s_btv_laeuft"), "{texts:?}");
        assert!(page.requested.is_some());
        wire.sync.pass_now().expect("pass");
        let (_, texts) = drawn(&mut page, 900.0);
        assert!(page.requested.is_none(), "the pass is over");
        assert!(!shows_alone(&texts, "s_btv_laeuft"));
        let _ = std::fs::remove_dir_all(dir);
    }

    /// The Basetool ends the connection: the page says why instead of only
    /// "not connected".
    #[test]
    fn an_ended_connection_names_its_reason() {
        let (mut page, dir) = page("beendet");
        let wire = wire_of(&page);
        let ctx = egui::Context::default();
        page.apply(Act::Switch(SETTING_ORGSTOCK, true), &ctx, 0.0);
        connect(&mut page);
        wire.sync.pass_now().expect("pass");
        wire.demo.as_ref().expect("demo").script_service_error(Some((401, "CLIENT_REVOKED")));
        assert!(wire.sync.pass_now().is_err());
        assert!(!wire.sync.connection().connected(), "a revoked client loses its token");
        let (_, texts) = drawn(&mut page, 900.0);
        assert!(shows(&texts, "s_btv_f_getrennt"), "{texts:?}");
        assert!(shows(&texts, "s_btv_nicht_verbunden"));
        let _ = std::fs::remove_dir_all(dir);
    }

    /// A right that is missing: the hint and the button for a new login; once
    /// it is granted the lines of the area come.
    #[test]
    fn a_missing_permission_offers_a_new_login() {
        let (mut page, dir) = page("rechte");
        let wire = wire_of(&page);
        let ctx = egui::Context::default();
        page.apply(Act::Switch(SETTING_ORGSTOCK, true), &ctx, 0.0);
        connect(&mut page);
        // The Basetool-side rights stay at what the first login granted, and
        // a second area is switched on in the settings file directly (its
        // page is not built, so the page would not let the switch go on).
        assert!(wire.sync.set_setting(SETTING_ORGSTOCK_WRITE, Value::Bool(true)));
        wire.sync.pass_now().expect("pass");
        let (_, texts) = drawn(&mut page, 900.0);
        assert!(shows(&texts, "s_btv_rechte_zusatz"), "the booking right is not granted yet");
        assert!(shows(&texts, "s_btv_rechte_erweitern"));
        page.apply(Act::Connect, &ctx, 0.0);
        assert!(wait_for(20, || wire
            .demo
            .as_ref()
            .and_then(|d| d.granted())
            .is_some_and(|g| g.iter().any(|s| s.ends_with("orgstock.write")))));
        wire.sync.pass_now().expect("pass");
        let (_, texts) = drawn(&mut page, 900.0);
        assert!(!shows(&texts, "s_btv_rechte_zusatz"));
        let _ = std::fs::remove_dir_all(dir);
    }

    /// A piece of the page drawn on its own: its texts and the acts it asked for.
    fn piece(width: f32, mut add: impl FnMut(&mut Ui, &Scheme, &mut Vec<Act>)) -> (f32, Vec<String>, Vec<Act>) {
        fn collect(shape: &egui::Shape, out: &mut Vec<String>) {
            match shape {
                egui::Shape::Text(text) => out.push(text.galley.text().to_lowercase()),
                egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| collect(s, out)),
                _ => {}
            }
        }
        let scheme = versekit_core::theme::current();
        let ctx = egui::Context::default();
        crate::install(&ctx);
        let (mut overflow, mut texts, mut acts) = (0.0, Vec::new(), Vec::new());
        for _ in 0..3 {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, 3000.0))),
                ..Default::default()
            };
            texts.clear();
            acts.clear();
            let mut output = ctx.run_ui(input, |ui| {
                overflow = crate::layout::page(ui, scheme, "seite", |p| add(p, scheme, &mut acts)).overflow;
            });
            output.shapes.iter().for_each(|clipped| collect(&clipped.shape, &mut texts));
            output.textures_delta.clear();
        }
        (overflow, texts, acts)
    }

    /// A change set the Basetool holds back: the link while it holds, the
    /// plain waiting without a link, and the expired note after the time.
    #[test]
    fn a_held_change_set_shows_its_button_and_its_expiry() {
        use versekit_core::basetool::scopes::{ROUTE_SHIPS, SETTING_SHIPS};
        let (page, dir) = page("gesperrt");
        let wire = wire_of(&page);
        let held = |until: f64| {
            Map::from([(
                "held".to_string(),
                Value::Object(vec![(ROUTE_SHIPS.into(), Value::Object(vec![("until".into(), Value::Float(until))]))]),
            )])
        };
        let now = wire.sync.now();
        let draw = |state: Map, at: f64| {
            piece(700.0, |ui, scheme, acts| held_note(ui, scheme, &wire, &state, SETTING_SHIPS, at, acts))
        };
        let (_, texts, _) = draw(held(now + 1000.0), now);
        assert!(
            shows(&texts, "s_btv_gesperrt") && !shows_alone(&texts, "s_btv_masse_oeffnen"),
            "no link yet: {texts:?}"
        );
        wire.sync.note_confirmation(ROUTE_SHIPS, Some("https://beispiel.profit-base.invalid/bestaetigen/x".into()));
        let (overflow, texts, _) = draw(held(now + 1000.0), now);
        assert_eq!(overflow, 0.0);
        assert!(shows(&texts, "s_btv_masse_h") && shows_alone(&texts, "s_btv_masse_oeffnen"), "{texts:?}");
        let (_, texts, _) = draw(held(now - 10.0), now);
        assert!(shows(&texts, "s_btv_gesperrt_alt") && !shows_alone(&texts, "s_btv_masse_oeffnen"), "{texts:?}");
        let (_, texts, _) = draw(Map::new(), now);
        assert!(texts.is_empty(), "nothing held, nothing said: {texts:?}");
        let _ = std::fs::remove_dir_all(dir);
    }

    /// The unit for new ships: only with more than one unit, and a unit the
    /// member left is set back to "as before".
    #[test]
    fn the_unit_for_new_ships_follows_the_member_list() {
        let (page, dir) = page("einheit");
        let wire = wire_of(&page);
        let units = |n: usize| -> Vec<OrgUnit> {
            (0..n)
                .map(|i| OrgUnit {
                    id: format!("u{i}"),
                    name: format!("Einheit {i}"),
                    shorthand: String::new(),
                    kind: String::new(),
                })
                .collect()
        };
        wire.units.publish(units(1));
        let (_, texts, _) = piece(700.0, |ui, scheme, acts| new_ship_unit(ui, scheme, &wire, &frame_of(&wire), acts));
        assert!(texts.is_empty(), "one unit: nothing to choose");
        wire.units.publish(units(2));
        let (overflow, texts, acts) =
            piece(420.0, |ui, scheme, acts| new_ship_unit(ui, scheme, &wire, &frame_of(&wire), acts));
        assert_eq!(overflow, 0.0);
        assert!(shows_alone(&texts, "s_btv_einheit_neu") && shows(&texts, "s_btv_einheit_wie_bisher"), "{texts:?}");
        assert!(acts.is_empty());
        assert!(wire.sync.set_setting(SETTING_NEW_SHIP_UNIT, Value::str("u9")));
        let (_, _, acts) = piece(700.0, |ui, scheme, acts| new_ship_unit(ui, scheme, &wire, &frame_of(&wire), acts));
        assert!(matches!(acts.as_slice(), [Act::NewShipUnit(id)] if id.is_empty()), "left unit goes back to as before");
        assert!(wire.sync.set_setting(SETTING_NEW_SHIP_UNIT, Value::str("u1")));
        let (_, texts, acts) =
            piece(700.0, |ui, scheme, acts| new_ship_unit(ui, scheme, &wire, &frame_of(&wire), acts));
        assert!(acts.is_empty() && texts.iter().any(|t| t == "einheit 1"), "{texts:?}");
        let _ = std::fs::remove_dir_all(dir);
    }

    /// The real connection is made without going out: no token, the file
    /// hint only where there is no keyring, and the stores are the ones of
    /// the pages.
    #[test]
    fn the_real_connection_is_made_without_going_out() {
        use versekit_core::legacy::{ENV_BASETOOL_KEYNAME, ENV_HOME, ENV_NO_NET, ENV_SECRETS, ENV_SECRETS_FILE};
        let dir = folder("echt");
        let secrets = format!("{dir}-geheim");
        // A made-up computer: nothing of the shell the check runs in counts.
        let vars = [
            (ENV_HOME, dir.as_str()),
            (ENV_SECRETS, secrets.as_str()),
            (ENV_SECRETS_FILE, "1"),
            (ENV_NO_NET, "1"),
            (ENV_BASETOOL_KEYNAME, "VerseKit Pruefung"),
        ];
        let host = versekit_core::paths::Host::new(versekit_core::pypath::Flavor::native(), &vars, &dir);
        let paths = Arc::new(Paths::new(host));
        let wire = Wire::system(paths, Arc::new(|| Some("PlayerOne".to_string())), "5.0.0").expect("wire");
        assert!(wire.demo.is_none(), "no made-up Basetool");
        assert!(!wire.sync.connection().connected());
        assert_eq!(wire.file_store.is_some(), !cfg!(windows), "the hint where the credentials sit in a file");
        assert!(!wire.sync.connection().config().sandbox);
        assert!(wire.sync.has_area(overview::COUNTS_ORGSTOCK));
        assert!(!wire.sync.area_visible(SETTING_ORGSTOCK));
        let mut state = ConnectionState::new(Some(wire.clone()));
        let (_, texts) = drawn(&mut state, 700.0);
        assert!(shows(&texts, "s_btv_nicht_verbunden") && !shows(&texts, "s_btv_beispiel"));
        drop(state);
        let _ = std::fs::remove_dir_all(dir);
        let _ = std::fs::remove_dir_all(secrets);
    }

    /// The connection of the program on a copy of the data (`--daten`): `vars` more variables of its host.
    /// Nothing can go out (`SC_BP_NO_NET`), the key has a name of its own.
    fn copy_wire(name: &str, vars: &[(&str, &str)]) -> (Wire, String) {
        use versekit_core::legacy::{ENV_BASETOOL_KEYNAME, ENV_NO_NET, ENV_SECRETS, ENV_SECRETS_FILE};
        let root = folder(name);
        std::fs::create_dir_all(std::path::Path::new(&root).join(versekit_core::paths::COPY_DATA_FOLDER)).unwrap();
        let secrets = format!("{root}/geheim");
        let mut all = vec![
            (ENV_SECRETS, secrets.as_str()),
            (ENV_SECRETS_FILE, "1"),
            (ENV_NO_NET, "1"),
            (ENV_BASETOOL_KEYNAME, "VerseKit Pruefung"),
        ];
        all.extend_from_slice(vars);
        let host = versekit_core::paths::Host::new(versekit_core::pypath::Flavor::native(), &all, &root);
        let paths = Paths::for_copy(host, &root).expect("a copy");
        let wire = Wire::system(Arc::new(paths), Arc::new(|| Some("PlayerOne".to_string())), "5.0.0").expect("wire");
        (wire, root)
    }

    /// On a copy of the data the connection is locked and the page says so; the sync button does nothing.
    /// Opened for a test, the note is gone.
    #[test]
    fn a_copy_of_the_data_locks_the_basetool_and_the_page_says_so() {
        let (wire, root) = copy_wire("testbetrieb", &[]);
        assert!(wire.sync.connection().test_locked(), "a copy of the data is locked");
        assert!(wire.sync.set_setting(SETTING_STOCK, Value::Bool(true)));
        let mut state = ConnectionState::new(Some(wire.clone()));
        let (overflow, texts) = drawn(&mut state, 420.0);
        assert!(shows(&texts, "s_btv_testbetrieb"), "{texts:?}");
        assert_eq!(overflow, 0.0, "the note wraps in a narrow column");
        // A sync asked for anyway ends at the lock, without the network.
        wire.sync.request_now();
        assert_eq!(wire.sync.pass_now().expect_err("locked").code, versekit_core::testmode::BASETOOL_LOCKED);
        drop(state);
        let _ = std::fs::remove_dir_all(root);
        // Counter-check: the variable for a test opens it, and the note goes.
        let (opened, root) = copy_wire("testbetrieb-offen", &[(versekit_core::testmode::ENV_BASETOOL_IN_TEST, "1")]);
        assert!(!opened.sync.connection().test_locked());
        let mut state = ConnectionState::new(Some(opened));
        let (_, texts) = drawn(&mut state, 420.0);
        assert!(!shows(&texts, "s_btv_testbetrieb"), "{texts:?}");
        drop(state);
        let _ = std::fs::remove_dir_all(root);
    }

    /// On a copy of the data the line of the test mode stands once, also after a sync or a login ended at the lock.
    #[test]
    fn the_line_of_the_test_mode_stands_once_after_a_sync_and_a_login_ended_at_the_lock() {
        use versekit_core::language::{Lang, t_in};
        let _language = crate::language_lock();
        let (wire, root) = copy_wire("testbetrieb-einmal", &[]);
        assert!(wire.sync.set_setting(SETTING_STOCK, Value::Bool(true)));
        let count = |texts: &[String]| {
            let wanted = [Lang::De, Lang::En].map(|lang| t_in(lang, "s_btv_testbetrieb").to_lowercase());
            texts.iter().filter(|shown| wanted.iter().any(|w| shown.contains(w.as_str()))).count()
        };
        wire.sync.request_now();
        assert_eq!(wire.sync.pass_now().expect_err("locked").code, versekit_core::testmode::BASETOOL_LOCKED);
        let mut state = ConnectionState::new(Some(wire.clone()));
        let (_, texts) = drawn(&mut state, 900.0);
        assert!(shows(&texts, "s_btv_nicht_verbunden"), "{texts:?}");
        assert_eq!(count(&texts), 1, "{texts:?}");
        // A login asked for anyway ends at the lock too.
        state.apply(Act::Connect, &egui::Context::default(), 0.0);
        assert!(wait_for(20, || matches!(wire.sync.login_state().get().1.as_deref(), Some(Login::Failed(_)))));
        let (_, texts) = drawn(&mut state, 900.0);
        assert_eq!(count(&texts), 1, "{texts:?}");
        drop(state);
        let _ = std::fs::remove_dir_all(root);
    }

    /// The write guard of a test (`VERSEKIT_BASETOOL_WRITE_MAX`) is said in a yellow line on the page: how many
    /// changes this run may send and how many are left, or that it only fetches. Without the variable the line
    /// is not there.
    #[test]
    fn the_write_guard_is_said_on_the_page_and_without_it_nothing_is_said() {
        use versekit_core::testmode::{ENV_BASETOOL_IN_TEST, ENV_BASETOOL_WRITE_MAX};
        let line = |texts: &[String]| texts.iter().any(|t| t.contains("prüfbetrieb") || t.contains("check mode"));
        // An odd ceiling no other check uses: the budget of the run is this test's alone.
        for (value, only_fetching) in [("0", true), ("9203", false)] {
            let (wire, root) = copy_wire("wache", &[(ENV_BASETOOL_IN_TEST, "1"), (ENV_BASETOOL_WRITE_MAX, value)]);
            assert_eq!(wire.sync.connection().write_guard(), Some(value.parse().unwrap()));
            let mut state = ConnectionState::new(Some(wire));
            let (overflow, texts) = drawn(&mut state, 420.0);
            assert_eq!(overflow, 0.0, "the line wraps in a narrow column");
            if only_fetching {
                assert!(shows(&texts, "s_btv_schreibwache_holen"), "{texts:?}");
            } else {
                assert!(
                    texts.iter().any(|t| (t.contains("höchstens 9203 änderung(en)") && t.contains("übrig: 9203"))
                        || (t.contains("at most 9203 change(s)") && t.contains("left: 9203"))),
                    "{texts:?}"
                );
            }
            drop(state);
            let _ = std::fs::remove_dir_all(root);
        }
        // Opened without the variable the copy only fetches, and the line says so.
        let (wire, root) = copy_wire("wache-fehlt", &[(ENV_BASETOOL_IN_TEST, "1")]);
        assert_eq!(wire.sync.connection().write_guard(), Some(0));
        let mut state = ConnectionState::new(Some(wire));
        let (_, texts) = drawn(&mut state, 420.0);
        assert!(shows(&texts, "s_btv_schreibwache_holen"), "{texts:?}");
        drop(state);
        let _ = std::fs::remove_dir_all(root);
        // Counter-check: the player's own data without the variable: no guard, no line.
        let (wire, dir, secrets) = own_wire("wache-eigen", &[(ENV_BASETOOL_IN_TEST, "1")]);
        assert_eq!(wire.sync.connection().write_guard(), None);
        let mut state = ConnectionState::new(Some(wire));
        let (_, texts) = drawn(&mut state, 420.0);
        assert!(!line(&texts), "{texts:?}");
        drop(state);
        let _ = std::fs::remove_dir_all(dir);
        let _ = std::fs::remove_dir_all(secrets);
    }

    /// The connection of the program on the player's own data, in a throwaway folder: `vars` more variables.
    /// Nothing can go out (`SC_BP_NO_NET`).
    fn own_wire(name: &str, vars: &[(&str, &str)]) -> (Wire, String, String) {
        use versekit_core::legacy::{ENV_BASETOOL_KEYNAME, ENV_HOME, ENV_NO_NET, ENV_SECRETS, ENV_SECRETS_FILE};
        let dir = folder(name);
        let secrets = format!("{dir}-geheim");
        let mut all = vec![
            (ENV_HOME, dir.as_str()),
            (ENV_SECRETS, secrets.as_str()),
            (ENV_SECRETS_FILE, "1"),
            (ENV_NO_NET, "1"),
            (ENV_BASETOOL_KEYNAME, "VerseKit Pruefung"),
        ];
        all.extend_from_slice(vars);
        let host = versekit_core::paths::Host::new(versekit_core::pypath::Flavor::native(), &all, &dir);
        let wire = Wire::system(Arc::new(Paths::new(host)), Arc::new(|| Some("PlayerOne".to_string())), "5.0.0")
            .expect("wire");
        (wire, dir, secrets)
    }

    /// Every connection the program makes in one run (after "Disconnect", a new window) draws on the one budget
    /// of the run: a change taken by the first is gone for the second.
    #[test]
    fn every_connection_of_a_run_draws_on_one_budget() {
        use versekit_core::testmode::{ENV_BASETOOL_IN_TEST, ENV_BASETOOL_WRITE_MAX, WRITE_GUARD};
        // An odd ceiling no other check uses: the budget of the run is this test's alone.
        let vars = [(ENV_BASETOOL_IN_TEST, "1"), (ENV_BASETOOL_WRITE_MAX, "9311")];
        let (first, root) = copy_wire("budget-erste", &vars);
        let conn = first.sync.connection().clone();
        let one = Value::Object(vec![("ops".into(), Value::Array(vec![Value::str("add")]))]);
        // Not connected, so it fails on the way, but past the guard: the change is taken.
        let sent = conn.request("POST", "/me/blueprints/changes", Some(&one), &[], Some("k"));
        assert!(sent.err().is_none_or(|e| e.code != WRITE_GUARD));
        assert_eq!(conn.write_left(), Some(9310));
        first.sync.stop();
        drop(first);
        let _ = std::fs::remove_dir_all(root);
        let (second, root) = copy_wire("budget-zweite", &vars);
        assert_eq!(second.sync.connection().write_left(), Some(9310), "a new connection counts on");
        second.sync.stop();
        let _ = std::fs::remove_dir_all(root);
    }

    /// On a copy no name of its own is the name of the test, never the one of the player's own program.
    #[test]
    fn a_copy_names_its_installation_for_the_test() {
        let (wire, root) = copy_wire("name-kopie", &[(versekit_core::testmode::ENV_BASETOOL_IN_TEST, "1")]);
        assert_eq!(wire.sync.label(), "VerseKit Echttest");
        let mut page = ConnectionState::new(Some(wire.clone()));
        page.label = String::new();
        page.keep_label(&wire);
        assert_eq!(page.label, "VerseKit Echttest", "an emptied field goes back to the name of the test");
        wire.sync.stop();
        drop(page);
        let _ = std::fs::remove_dir_all(root);
        let (own, dir, secrets) = own_wire("name-eigen", &[]);
        assert_eq!(own.sync.label(), config::default_label(), "own data keep the name of the system");
        own.sync.stop();
        let _ = std::fs::remove_dir_all(dir);
        let _ = std::fs::remove_dir_all(secrets);
    }

    /// A copy opened for a test that signs in with the key of the real installation keeps the key and says so;
    /// with a key name of its own the key is its own and the line is not there.
    #[test]
    fn a_copy_with_the_real_key_keeps_it_and_the_page_says_so() {
        use versekit_core::legacy::ENV_BASETOOL_KEYNAME;
        use versekit_core::testmode::ENV_BASETOOL_IN_TEST;
        let real = versekit_core::basetool::key::KEY_NAME;
        let (wire, root) = copy_wire("schluessel-echt", &[(ENV_BASETOOL_IN_TEST, "1"), (ENV_BASETOOL_KEYNAME, real)]);
        assert!(wire.sync.connection().key_kept());
        let mut state = ConnectionState::new(Some(wire));
        let (overflow, texts) = drawn(&mut state, 420.0);
        assert!(shows(&texts, "s_btv_schluessel_bleibt"), "{texts:?}");
        assert_eq!(overflow, 0.0, "the line wraps in a narrow column");
        drop(state);
        let _ = std::fs::remove_dir_all(root);
        // Counter-check: the key name of its own the checks give.
        let (wire, root) = copy_wire("schluessel-eigen", &[(ENV_BASETOOL_IN_TEST, "1")]);
        assert!(!wire.sync.connection().key_kept());
        let mut state = ConnectionState::new(Some(wire));
        let (_, texts) = drawn(&mut state, 420.0);
        assert!(!shows(&texts, "s_btv_schluessel_bleibt"), "{texts:?}");
        drop(state);
        let _ = std::fs::remove_dir_all(root);
    }

    /// Nothing sticks out: unlinked, with the code, connected with all cards,
    /// narrow and wide, both languages.
    #[test]
    fn nothing_sticks_out_in_any_state() {
        use versekit_core::language::{Lang, with_language};
        let (mut page, dir) = page("rand");
        let wire = wire_of(&page);
        let ctx = egui::Context::default();
        // The language is this thread's only: other tests go on in theirs.
        let check = |page: &mut ConnectionState, what: &str| {
            let mut failed = None;
            for language in [Lang::En, Lang::De] {
                with_language(language, || {
                    for width in [300.0_f32, 420.0, 700.0, 1400.0] {
                        let (overflow, _) = drawn(page, width);
                        if overflow != 0.0 && failed.is_none() {
                            failed = Some(format!("{what}, {language:?}, {width}: {overflow}"));
                        }
                    }
                });
            }
            assert_eq!(failed, None);
        };
        check(&mut page, "unlinked");
        page.apply(Act::Switch(SETTING_ORGSTOCK, true), &ctx, 0.0);
        page.apply(Act::Connect, &ctx, 0.0);
        assert!(wait_for(10, || matches!(wire.sync.login_state().get().1.as_deref(), Some(Login::Code { .. }))));
        check(&mut page, "code");
        assert!(wait_for(20, || wire.sync.connection().connected()));
        wire.sync.pass_now().expect("pass");
        wire.sync.decide(Arc::new(|state: &mut Map| {
            map_set(state, "conflicts", Value::Object(vec![("k".into(), Value::str("bt"))]));
            map_set(
                state,
                "stock",
                Value::Object(vec![(
                    "conflicts".into(),
                    Value::Object(vec![("a".into(), Value::Array(vec![Value::int(1), Value::int(2)]))]),
                )]),
            );
        }));
        page.state_stale = true;
        check(&mut page, "connected");
        let _ = std::fs::remove_dir_all(dir);
        // The account check stops a first pass: its card, with a wrong account.
        let (mut stopped, dir) = self::page("rand-konto");
        let wire = wire_of(&stopped);
        wire.demo.as_ref().expect("demo").script_account("mismatch");
        stopped.apply(Act::Switch(SETTING_ORGSTOCK, true), &ctx, 0.0);
        connect(&mut stopped);
        wire.sync.pass_now().expect("pass");
        assert!(wire.sync.status().get().1.is_some_and(|s| s.code == "account_mismatch"));
        check(&mut stopped, "account");
        let _ = std::fs::remove_dir_all(dir);
    }

    /// A piece of the page driven by the keyboard: `tabs` times Tab, then
    /// Enter; the acts of the frame with Enter.
    fn keyed(width: f32, tabs: usize, mut add: impl FnMut(&mut Ui, &Scheme, &mut Vec<Act>)) -> Vec<Act> {
        let scheme = versekit_core::theme::current();
        let ctx = egui::Context::default();
        crate::install(&ctx);
        let key = |key: egui::Key| egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::default(),
        };
        let mut found = Vec::new();
        for step in 0..tabs + 2 {
            let events = match step {
                0 => Vec::new(),
                s if s <= tabs => vec![key(egui::Key::Tab)],
                _ => vec![key(egui::Key::Enter)],
            };
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, 3000.0))),
                events,
                ..Default::default()
            };
            found.clear();
            let mut output = ctx.run_ui(input, |ui| {
                crate::layout::page(ui, scheme, "seite", |p| add(p, scheme, &mut found));
            });
            output.textures_delta.clear();
        }
        found
    }

    /// ⚠⚠ Whatever is activated in the code box carries the code or the bare
    /// address of the login, never a link with the code built in: tried by
    /// walking through every control with the keyboard.
    #[test]
    fn the_code_box_copies_and_opens_only_the_code_and_the_bare_address() {
        let mut page = ConnectionState::new(None);
        let (code, address) = ("DEMO-4711", "https://beispiel.profit-base.invalid/auth/realms/iri/device");
        let mut all = Vec::new();
        for tabs in 1..=7 {
            all.extend(keyed(900.0, tabs, |ui, scheme, acts| login_box(ui, scheme, &mut page, code, address, acts)));
        }
        let browser = all.iter().filter(|a| matches!(a, Act::Browser(url) if url == address)).count();
        assert!(browser >= 2, "the link and the button open the bare address: {}", all.len());
        assert!(all.iter().any(|a| matches!(a, Act::Copy(Copied::Code, text) if text == code)));
        assert!(all.iter().any(|a| matches!(a, Act::Copy(Copied::Address, text) if text == address)));
        assert!(all.iter().any(|a| matches!(a, Act::Cancel)));
        for act in &all {
            match act {
                Act::Browser(text) | Act::Copy(_, text) => {
                    assert!(text == code || text == address, "an act carries something else: {text}");
                }
                Act::Cancel => {}
                _ => panic!("the code box asked for something else"),
            }
        }
    }

    /// The demand, the unit storage and the list of units come with the sync, each only with its switch on.
    #[test]
    fn a_pass_fetches_the_units_the_unit_storage_and_the_demand_each_with_its_switch() {
        let (mut page, dir) = page("holen");
        let wire = wire_of(&page);
        let ctx = egui::Context::default();
        // Nothing on, nothing fetched; the rights of the login are those of the areas that are on.
        page.apply(Act::Switch(SETTING_DEMAND, true), &ctx, 0.0);
        page.apply(Act::Switch(SETTING_ORGSTOCK, true), &ctx, 0.0);
        page.apply(Act::Connect, &ctx, 0.0);
        assert!(wait_for(10, || matches!(wire.sync.login_state().get().1.as_deref(), Some(Login::Code { .. }))));
        assert!(wait_for(20, || wire.sync.connection().connected()));
        let now = wire.sync.now();
        assert!(wire.demand.current(now).1.is_none() && wire.org_stock.current(now).1.is_none(), "before the pass");
        wire.sync.pass_now().expect("pass");
        assert!(wire.demand.current(now).1.is_some(), "the demand came with the pass");
        assert!(wire.org_stock.current(now).1.is_some(), "the unit storage came with the pass");
        // Off again: what the area held is thrown away.
        page.apply(Act::Switch(SETTING_DEMAND, false), &ctx, 0.0);
        assert!(wait_for(20, || wire.demand.current(now).1.is_none()), "the demand is gone with its switch");
        drop(page);
        let _ = std::fs::remove_dir_all(dir);
    }

    /// The name of the connection (the one under "Connected applications") goes into the settings of the
    /// program, only when it is valid; an empty one is the default name again.
    #[test]
    fn the_name_of_the_connection_goes_into_the_settings_only_when_it_is_valid() {
        let (mut page, dir) = page("name");
        let wire = wire_of(&page);
        let ctx = egui::Context::default();
        let saved = || wire.sync.paths().setting(SETTING_LABEL);
        page.label = "Mein Rechner".into();
        page.apply(Act::Label, &ctx, 0.0);
        assert_eq!(saved().as_deref(), Some("Mein Rechner"));
        assert_eq!(wire.sync.label(), "Mein Rechner");
        assert!(!page.label_error);
        for wrong in ["Name mit /", "Zu lang ".repeat(6).as_str(), "-Strich", "Ä.ö-_ 1"] {
            page.label = wrong.to_string();
            page.apply(Act::Label, &ctx, 0.0);
            let valid = config::label_valid(wrong.trim());
            assert_eq!(page.label_error, !valid, "{wrong:?}");
            assert_eq!(saved().as_deref(), Some(if valid { wrong.trim() } else { "Mein Rechner" }), "{wrong:?}");
            if valid {
                page.label = "Mein Rechner".into();
                page.apply(Act::Label, &ctx, 0.0);
            }
        }
        // Empty: the default name comes back, in the field and in the settings.
        page.label = "   ".into();
        page.apply(Act::Label, &ctx, 0.0);
        assert_eq!(page.label, config::default_label());
        assert_eq!(wire.sync.label(), config::default_label());
        drop(page);
        let _ = std::fs::remove_dir_all(dir);
    }

    /// The name field takes the saved name again only when nothing is typed
    /// and the field did not lose the focus in this very frame.
    #[test]
    fn a_name_just_typed_is_not_overwritten_as_the_field_loses_focus() {
        assert!(reload_label(false, false, false), "idle: show the saved name");
        assert!(!reload_label(true, true, false), "typing");
        assert!(!reload_label(false, true, false), "the frame in which Tab took the focus away");
        assert!(!reload_label(false, false, true), "while the error of a wrong name stands");
    }
}
