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

//! The unit storage page (`orgstock_page.py`): the storage of the own units
//! from the KRT Profit Basetool (`versekit_core::basetool::orgstock`).
//!
//! On top the unit (when there are several), the tabs Material and Items as
//! in My stock, the filters kind and place and the search. Below the lots,
//! one row each. A logistician of the unit opens a lot and sees one entry
//! per owner; with booking on every entry has its six bookings as icons, and
//! the tab row offers booking in.
//!
//! ⚠ **Forms stand in the list, no window of their own**: a window above
//! would take the focus and hide the very entry it is about.
//!
//! ⚠ **Requests run in their own thread**; after a booking the feed is
//! fetched. **New data never resets the page**: form, what was typed, open
//! lots, filters, search and scroll position stay; only what no longer
//! exists goes.

use versekit_core::language::SpawnWithLanguage as _;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;
use std::sync::mpsc::Receiver;
use std::time::Duration;

use egui::accesskit::Role;
use egui::{Align, Key, Layout, Rect, RichText, Sense, Ui, UiBuilder, WidgetInfo, vec2};
use versekit_core::basetool::connection::{Clock, Connection};
use versekit_core::basetool::error::{ApiError, error_text};
use versekit_core::basetool::feed::{LOCATIONS_AGAIN, Locations};
use versekit_core::basetool::hold::CONFIRM_LIFETIME;
use versekit_core::basetool::orgstock::{
    self, AmountError, Earmarks, Entry, Found, Lot, OrgStock, OrgStockData, Outcome, PIECE, Resolved, SCU, Targets,
};
use versekit_core::basetool::overview::Verdict;
use versekit_core::basetool::scopes::SETTING_ORGSTOCK;
use versekit_core::basetool::sync::{Phase, SyncHandle};
use versekit_core::formats;
use versekit_core::jsonio::Value;
use versekit_core::language::{self, Arg, t, tf};
use versekit_core::paths::Paths;
use versekit_core::stock;
use versekit_core::stock_data::{ITEM_KINDS, Kind, MATERIAL_KINDS, StockData, lower};
use versekit_core::theme::{Color, IconTint, Scheme};

use crate::color;
use crate::layout::GAP;
use crate::widgets::{self, ButtonKind};

const SETTING_UNIT: &str = "staffellager_einheit";
const SETTING_VIEW: &str = "staffellager_sicht";
/// Width of the column with the arrow (lots) or the six icons (entries).
const ACTIONS_WIDTH: f32 = 6.0 * (widgets::ICON_BUTTON_SIZE + 4.0) + 4.0;
/// Below this room for the name a row goes on two lines.
const NAME_MIN: f32 = 170.0;
const INDENT: f32 = 24.0;
/// How often the made-up Basetool of the preview syncs by itself.
const DEMO_CADENCE: f64 = 60.0;

/// The bookings at an entry: kind, Lucide icon, meaning.
const ENTRY_ACTIONS: [(Action, &str, &str); 6] = [
    (Action::BookOut, "package-minus", "s_sl_ausbuchen"),
    (Action::Sell, "hand-coins", "s_sl_verkaufen"),
    (Action::Move, "arrow-right-left", "s_sl_verschieben"),
    (Action::Note, "sticky-note", "s_sl_notiz"),
    (Action::Stolen, "shield-alert", "s_sl_marke"),
    (Action::Earmark, "bookmark", "s_sl_reservieren"),
];

/// One booking at an entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    BookOut,
    Sell,
    Move,
    Note,
    Stolen,
    Earmark,
}

/// The open form: booking in at the top, or one at an entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FormKey {
    BookIn(String),
    Entry(Action, String),
}

/// Where a move goes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MoveMode {
    #[default]
    Location,
    Holder,
    Pool,
}

/// What a form holds; it stays while new data comes in.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Values {
    pub holder: String,
    pub material: String,
    pub location: String,
    pub piece: bool,
    pub amount: String,
    pub quality: String,
    pub merge: bool,
    pub stolen: bool,
    pub note: String,
    pub terminal: String,
    pub proceeds: String,
    pub mode: MoveMode,
    pub to_unit: String,
    /// Amounts taken from reservations: `job:<id>` / `mission:<id>`.
    pub from: BTreeMap<String, String>,
    /// New amounts of existing reservations: `job:<id>` / `mission:<id>`.
    pub change: BTreeMap<String, String>,
    /// The version of the entry when the form opened.
    pub version: Option<i64>,
    pub new_target: String,
    pub new_amount: String,
    /// Written by the lookup, not by the player.
    pub material_ref: Option<Found>,
    pub candidates: Vec<Found>,
    pub candidates_for: String,
}

impl Values {
    /// What the player typed, without what the lookup wrote.
    fn typed(&self) -> Values {
        Values { material_ref: None, candidates: Vec::new(), candidates_for: String::new(), ..self.clone() }
    }
}

/// The tone of a message line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    Quiet,
    Done,
    Warn,
    Error,
}

/// What reaches the page: the store, the connection, the sync.
#[derive(Clone)]
pub struct Link {
    pub store: Arc<OrgStock>,
    pub conn: Arc<Connection>,
    /// The sync of the program; without it (preview, tests) the page fetches itself.
    pub sync: Option<SyncHandle>,
    pub clock: Clock,
    /// The own game account, to choose the own entry as owner.
    pub own_account: Option<String>,
    /// The made-up Basetool of the preview: syncs by itself every minute,
    /// opens no browser.
    pub demo: bool,
}

impl Link {
    /// The page on the sync of the program: the store the sync fills, the
    /// connection it uses.
    pub fn from_wire(wire: &crate::basetool_page::Wire) -> Link {
        Link {
            store: wire.org_stock.clone(),
            conn: wire.sync.connection().clone(),
            sync: Some(wire.sync.clone()),
            clock: versekit_core::basetool::connection::system_clock(),
            own_account: wire.own_account.clone(),
            demo: wire.demo.is_some(),
        }
    }

    /// The page against the made-up Basetool of the preview.
    pub fn demo() -> Link {
        Link::demo_with().0
    }

    /// Like [`Link::demo`], with the made-up Basetool itself (tests look at
    /// what reached it).
    pub fn demo_with() -> (Link, Arc<versekit_core::basetool::demo::DemoBasetool>) {
        let demo = Arc::new(versekit_core::basetool::demo::DemoBasetool::new());
        let clock = versekit_core::basetool::connection::system_clock();
        let link = Link {
            store: OrgStock::new(),
            conn: Arc::new(demo.connection(clock.clone())),
            sync: None,
            clock,
            own_account: Some("PlayerOne".into()),
            demo: true,
        };
        (link, demo)
    }
}

/// A request in its own thread.
struct Pending<T> {
    answer: Receiver<T>,
}

impl<T> Pending<T> {
    /// `None` while it runs, `Some(None)` when its thread ended without an
    /// answer (it could not start, or it broke off).
    fn poll(&self) -> Option<Option<T>> {
        match self.answer.try_recv() {
            Ok(value) => Some(Some(value)),
            Err(std::sync::mpsc::TryRecvError::Empty) => None,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => Some(None),
        }
    }

    /// Waits (tests only).
    #[cfg(test)]
    fn wait(&self) -> Option<T> {
        self.answer.recv_timeout(Duration::from_secs(10)).ok()
    }
}

fn spawn<T: Send + 'static>(ctx: Option<&egui::Context>, work: impl FnOnce() -> T + Send + 'static) -> Pending<T> {
    let (send, answer) = std::sync::mpsc::channel();
    let ctx = ctx.cloned();
    let started = std::thread::Builder::new().name("lager-der-einheit".into()).spawn_with_language(move || {
        let _ = send.send(work());
        if let Some(ctx) = ctx {
            ctx.request_repaint();
        }
    });
    if started.is_err() {
        versekit_core::errors::record("orgstock_page.thread", &"no thread");
    }
    Pending { answer }
}

/// What a request of the page brings back.
enum Answer {
    /// A fetch of the store.
    Fetched(Result<(), ApiError>),
    /// A change set: what became of it, then the fetch after it.
    Sent { form: u64, outcome: Result<Outcome, ApiError> },
    /// A lookup of goods for booking in.
    Looked { ticket: u64, form: FormKey, typed: Box<Values>, name: String, found: Result<Resolved, ApiError> },
}

/// Orders and missions of a pool for some goods, on their way.
type TargetsJob = ((String, String), Pending<Result<Targets, ApiError>>);

/// The page.
pub struct OrgStockState {
    link: Option<Link>,
    /// Why nothing may be booked (the real Basetool in a test environment); `None`: bookings go.
    book_lock: Option<String>,
    /// The page keeps unit and tab in the settings file.
    keeps_settings: bool,
    catalog: StockData,
    data: Option<Arc<OrgStockData>>,
    seen: u64,
    pub unit: String,
    unit_chosen: bool,
    pub items: bool,
    pub kind: Option<Kind>,
    pub place: String,
    pub search: String,
    pub open: BTreeSet<String>,
    pub form: Option<FormKey>,
    /// Counts the forms opened; a booking closes only the form it came from.
    form_serial: u64,
    pub values: Values,
    /// Errors at the fields: field name → sentence.
    pub errors: HashMap<&'static str, String>,
    pub message: Option<(String, Tone)>,
    /// A notice at the top right: text and when it came.
    notice: Option<(String, Option<f64>)>,
    /// The icon of the notice.
    notice_icon: &'static str,
    /// The sync button was pressed at this generation of the status and waits
    /// for the pass to end.
    sync_wait: Option<u64>,
    busy: Option<Pending<Answer>>,
    ticket: u64,
    fetching: Option<Pending<Answer>>,
    fetch_error: Option<String>,
    /// The confirmation page of a held booking and until when it holds.
    pub confirm: Option<(String, f64)>,
    locations: Option<(Vec<String>, f64)>,
    locations_loading: Option<Pending<Result<Vec<String>, ApiError>>>,
    targets: HashMap<(String, String), Option<Targets>>,
    targets_loading: Vec<TargetsJob>,
    last_fetch_asked: Option<f64>,
    /// The names the goods field offers, for the state of the storage seen.
    names: Option<(u64, Arc<Vec<String>>)>,
}

impl Default for OrgStockState {
    fn default() -> Self {
        OrgStockState::new(None, None)
    }
}

/// The text key of a refusal (`reason_text`).
pub fn reason_key(code: &str) -> Option<&'static str> {
    Some(match code {
        "NOT_PERMITTED" => "s_sl_r_not_permitted",
        "HOLDER_UNKNOWN" => "s_sl_r_holder_unknown",
        "VERSION_CONFLICT" => "s_sle_r_version_conflict",
        "INSUFFICIENT_STOCK" => "s_sl_r_insufficient_stock",
        "STOCK_EARMARKED" => "s_sl_r_stock_earmarked",
        "STOCK_OFFERED" => "s_sl_r_stock_offered",
        "OVER_ALLOCATION" => "s_sl_r_over_allocation",
        "EARMARK_TARGET_INVALID" => "s_sl_r_earmark_target_invalid",
        "UNIT_MISMATCH" => "s_sl_r_unit_mismatch",
        "LOCATION_UNKNOWN" => "s_sl_r_location_unknown",
        "STOLEN_MARKING_DISABLED" => "s_sl_r_stolen_marking_disabled",
        "UNMATCHED" => "s_sl_r_unmatched",
        "AMBIGUOUS" => "s_sle_r_ambiguous",
        _ => return None,
    })
}

pub fn reason_text(code: &str) -> String {
    match reason_key(code) {
        Some(key) => t(key),
        None => tf("s_sl_r_anders", &[("0", Arg::from(if code.is_empty() { "?" } else { code }))]),
    }
}

fn unit_word(piece: bool) -> String {
    if piece { t("s_lg_stueck") } else { "SCU".to_string() }
}

fn amount_text(value: f64, piece: bool) -> String {
    formats::amount(language::current(), value, &unit_word(piece))
}

/// A number for a field: up to three places, no thousands mark.
fn field_text(value: f64) -> String {
    let text = format!("{:.3}", stock::py_round(value, 3));
    let text = text.trim_end_matches('0').trim_end_matches('.');
    text.replace('.', &formats::decimal_mark(language::current()).to_string())
}

fn holder_of(entry: &Entry) -> String {
    if entry.withheld {
        t("s_sl_verborgen")
    } else if entry.holder.is_empty() {
        "?".into()
    } else {
        entry.holder.clone()
    }
}

fn job_label(number: Option<i64>) -> String {
    number.map_or_else(|| t("s_bd_auftrag"), |n| format!("#{n}"))
}

fn amount_error_text(error: AmountError, piece: bool) -> String {
    match error {
        AmountError::Missing => t("s_sl_menge_fehlt"),
        AmountError::Wrong => t("s_sl_menge_falsch"),
        AmountError::TooMuch(most) => tf("s_sl_menge_zu_viel", &[("0", Arg::from(amount_text(most, piece)))]),
    }
}

/// The section of the goods list as text (`stock_catalog.SECTION_KEYS`).
fn section_key(section: &str) -> &'static str {
    match section {
        "Armor" => "s_uk_armor",
        "Avionics" => "s_uk_avionics",
        "Personal Weapons" => "s_uk_personal_weapons_s",
        "Propulsion" => "s_uk_propulsion",
        "Systems" => "s_uk_systems",
        "Undersuits" => "s_uk_undersuits_s",
        "Utility" => "s_uk_utility",
        "Vehicle Weapons" => "s_uk_vehicle_weapons_s",
        "Clothing" => "s_sl_k_clothing",
        "Miscellaneous" => "s_sl_k_miscellaneous",
        "Liveries" => "s_sl_k_liveries",
        "Consumable" => "s_sl_k_consumable",
        "Technology" => "s_sl_k_technology",
        "Module" => "s_sl_k_module",
        "Decorations" => "s_sl_k_decorations",
        "Flair" => "s_sl_k_flair",
        _ => "s_sl_k_gegenstand",
    }
}

/// The class of a component as text; `None` for an unknown one.
fn class_text(code: &str) -> Option<String> {
    let key = match code {
        "MILITARY" => "s_ld_kl_military",
        "CIVILIAN" => "s_ld_kl_civilian",
        "INDUSTRIAL" => "s_ld_kl_industrial",
        "STEALTH" => "s_ld_kl_stealth",
        "COMPETITION" => "s_ld_kl_competition",
        _ => return None,
    };
    Some(t(key))
}

/// The kinds of components that are weapons.
const WEAPON_COMPONENTS: [&str; 8] =
    ["WEAPON_GUN", "WEAPON_MINING", "MISSILE_LAUNCHER", "MISSILE", "BOMB_LAUNCHER", "BOMB", "TURRET", "EMP"];

impl OrgStockState {
    /// The page; without a link it waits for the Basetool. `paths` is the
    /// data folder: goods list, kinds and the remembered unit and tab.
    pub fn new(link: Option<Link>, paths: Option<&Paths>) -> OrgStockState {
        let catalog = match paths {
            Some(p) => StockData::load(|name| p.app_file(name)),
            None => StockData::load(|_| String::new()),
        };
        let unit = paths.and_then(|p| p.setting(SETTING_UNIT)).unwrap_or_default();
        let items = paths.map(|p| orgstock::view_of(p.setting(SETTING_VIEW).as_deref())) == Some("items");
        OrgStockState {
            link,
            book_lock: None,
            keeps_settings: paths.is_some(),
            catalog,
            data: None,
            seen: 0,
            unit_chosen: !unit.is_empty(),
            unit,
            items,
            kind: None,
            place: String::new(),
            search: String::new(),
            open: BTreeSet::new(),
            form: None,
            form_serial: 0,
            values: Values::default(),
            errors: HashMap::new(),
            message: None,
            notice: None,
            notice_icon: "check",
            sync_wait: None,
            busy: None,
            ticket: 0,
            fetching: None,
            fetch_error: None,
            confirm: None,
            locations: None,
            locations_loading: None,
            targets: HashMap::new(),
            targets_loading: Vec::new(),
            last_fetch_asked: None,
            names: None,
        }
    }

    /// The settings this page keeps in the settings file.
    pub fn setting_values(&self) -> Vec<(&'static str, Value)> {
        if !self.keeps_settings {
            return Vec::new();
        }
        let mut values = vec![(SETTING_VIEW, Value::str(if self.items { "items" } else { "material" }))];
        // The units of the made-up Basetool never go into the settings.
        if self.unit_chosen && !self.link.as_ref().is_some_and(|l| l.demo) {
            values.push((SETTING_UNIT, Value::str(&self.unit)));
        }
        values
    }

    /// Whether a filter or the search narrows the list.
    pub fn is_narrowed(&self) -> bool {
        self.kind.is_some() || !self.place.is_empty() || !self.search.trim().is_empty()
    }

    /// Back to every lot: filters and search cleared.
    pub fn reset(&mut self) {
        self.kind = None;
        self.place.clear();
        self.search.clear();
    }

    /// How old the shown data is, in seconds; `None` without data.
    pub fn age(&self) -> Option<f64> {
        let link = self.link.as_ref()?;
        self.data.as_ref().map(|d| ((link.clock)() - d.at).max(0.0))
    }

    fn now(&self) -> f64 {
        self.link.as_ref().map_or(0.0, |l| (l.clock)())
    }

    /// Takes in what the sync brought since the page was last looked at, so the tile of the overview
    /// shows the stand without a visit to the page.
    pub fn follow_store(&mut self) {
        self.take_in();
    }

    pub fn data(&self) -> Option<&OrgStockData> {
        self.data.as_deref()
    }

    pub fn busy(&self) -> bool {
        self.busy.is_some()
    }

    fn say(&mut self, text: String, tone: Tone) {
        self.message = Some((text, tone));
    }

    fn tell(&mut self, text: String, icon: &'static str) {
        self.notice = Some((text, None));
        self.notice_icon = icon;
    }

    /// Takes new data in; the page's own state stays, only what no longer
    /// exists goes.
    fn take_in(&mut self) {
        let Some(link) = &self.link else { return };
        let (generation, data) = link.store.current((link.clock)());
        if generation == self.seen {
            return;
        }
        self.seen = generation;
        self.data = data;
        let Some(data) = self.data.clone() else { return };
        if !data.units.is_empty() && !data.units.iter().any(|u| u.id == self.unit) {
            // The chosen unit is gone: a form for it and a lookup end.
            self.drop_pending();
            self.unit = data.units[0].id.clone();
            // A stand-in, not a choice: the setting keeps what was chosen.
            self.unit_chosen = false;
            self.close_form();
        }
        match &self.form {
            Some(FormKey::BookIn(unit)) if !data.can_write(unit) => {
                self.drop_pending();
                self.close_form();
            }
            Some(FormKey::Entry(_, id)) => {
                let gone = match data.entry(id) {
                    None => true,
                    Some((lot, _)) => !data.can_write(&lot.unit_id),
                };
                if gone {
                    self.drop_pending();
                    self.close_form();
                    self.say(t("s_sle_eintrag_weg"), Tone::Warn);
                } else if self.values.version.is_none() {
                    self.values.version = data.entry(id).map(|(_, e)| e.version);
                }
            }
            Some(FormKey::BookIn(unit)) => {
                // An owner who left the member list: back to the own entry or the first.
                let roster = data.rosters.get(unit).cloned().unwrap_or_default();
                if !roster.iter().any(|m| m.holder_ref == self.values.holder) {
                    let own = orgstock::own_member(&roster, self.link.as_ref().and_then(|l| l.own_account.as_deref()));
                    self.values.holder = if own.is_empty() {
                        roster.first().map(|m| m.holder_ref.clone()).unwrap_or_default()
                    } else {
                        own
                    };
                }
            }
            _ => {}
        }
    }

    /// The sync button: fetches now, and once the pass is over a notice says
    /// how it went (differing amounts, a held change set, an error).
    pub fn sync_now(&mut self, ctx: Option<&egui::Context>) {
        // The generation is read before the pass is asked for: a pass that
        // ends at once must still count as later.
        let asked_at = self.link.as_ref().and_then(|l| l.sync.as_ref()).map(|sync| sync.status().generation());
        self.fetch_now(ctx);
        if asked_at.is_some() {
            self.sync_wait = asked_at;
            self.tell(t("s_btv_sync_laeuft"), "rotate-cw");
        }
    }

    /// Shows the result of the pass the sync button asked for, once it is over.
    fn show_sync_result(&mut self) {
        let Some(waiting_since) = self.sync_wait else { return };
        let Some(sync) = self.link.as_ref().and_then(|l| l.sync.clone()) else { return };
        let (generation, status) = sync.status().get();
        let Some(status) = status.filter(|s| generation > waiting_since && s.phase != Phase::Running) else { return };
        self.sync_wait = None;
        let state = sync.state().unwrap_or_default();
        let (text, verdict) =
            versekit_core::basetool::overview::sync_result(&status, &state, Some(SETTING_ORGSTOCK), sync.now());
        let icon = match verdict {
            Verdict::Done => "check",
            Verdict::Waiting => "clock",
            Verdict::Failed => "triangle-alert",
        };
        self.tell(text, icon);
    }

    /// Fetches now: through the sync of the program, or here.
    pub fn fetch_now(&mut self, ctx: Option<&egui::Context>) {
        let Some(link) = self.link.clone() else { return };
        self.last_fetch_asked = Some((link.clock)());
        if let Some(sync) = &link.sync {
            sync.request_now();
            return;
        }
        if self.fetching.is_some() {
            return;
        }
        let (details, write) = self.data.as_ref().map_or((true, true), |d| (d.details, d.write));
        let (details, write) = if link.demo { (true, true) } else { (details, write) };
        self.fetching = Some(spawn(ctx, move || {
            Answer::Fetched(link.store.refresh(&link.conn, details, write, (link.clock)()).map(|_| ()))
        }));
    }

    /// Looks at running requests and new data; the made-up Basetool syncs
    /// by itself.
    pub fn tick(&mut self, ctx: Option<&egui::Context>) {
        let lost = || ApiError::internal("INTERNAL", "request thread ended without an answer");
        self.show_sync_result();
        if let Some(answer) = self.fetching.as_ref().and_then(Pending::poll) {
            self.fetching = None;
            self.handle(answer.unwrap_or_else(|| Answer::Fetched(Err(lost()))));
        }
        if let Some(answer) = self.busy.as_ref().and_then(Pending::poll) {
            self.busy = None;
            match answer {
                Some(answer) => self.handle(answer),
                None => {
                    self.ticket = 0;
                    versekit_core::errors::record("orgstock_page.send", &lost());
                    self.say(error_text("INTERNAL"), Tone::Error);
                }
            }
        }
        if let Some(found) = self.locations_loading.as_ref().and_then(Pending::poll) {
            self.locations_loading = None;
            match found.unwrap_or_else(|| Err(lost())) {
                Ok(names) => self.locations = Some((names, self.now())),
                Err(error) => {
                    // Empty for a minute, then the field asks again.
                    versekit_core::errors::record("orgstock_page.locations", &error);
                    self.locations = Some((Vec::new(), self.now() - LOCATIONS_AGAIN + 60.0));
                }
            }
        }
        let mut done = Vec::new();
        for (i, (key, pending)) in self.targets_loading.iter().enumerate() {
            if let Some(found) = pending.poll() {
                done.push((i, key.clone(), found.unwrap_or_else(|| Err(lost()))));
            }
        }
        for (i, key, found) in done.into_iter().rev() {
            self.targets_loading.remove(i);
            match found {
                Ok(found) => {
                    self.targets.insert(key, Some(found));
                }
                Err(error) => {
                    // Shown as none this time; opening the form again asks again.
                    versekit_core::errors::record("orgstock_page.targets", &error);
                    self.targets.insert(key, Some(Targets::default()));
                }
            }
        }
        self.take_in();
        if let Some(link) = &self.link
            && link.demo
            && self.fetching.is_none()
        {
            let now = (link.clock)();
            if self.last_fetch_asked.is_none_or(|at| now - at >= DEMO_CADENCE) {
                self.fetch_now(ctx);
            }
        }
        if let Some(ctx) = ctx
            && (self.busy.is_some() || self.fetching.is_some() || self.locations_loading.is_some())
        {
            ctx.request_repaint_after(Duration::from_millis(200));
        }
        if let Some(ctx) = ctx
            && self.link.as_ref().is_some_and(|l| l.demo)
        {
            ctx.request_repaint_after(Duration::from_secs(5));
        }
    }

    fn handle(&mut self, answer: Answer) {
        match answer {
            Answer::Fetched(Ok(())) => self.fetch_error = None,
            Answer::Fetched(Err(error)) => {
                versekit_core::errors::record("orgstock_page.refresh", &error);
                self.fetch_error = Some(tf("s_sle_holen_fehler", &[("0", Arg::from(error_text(&error.code)))]));
            }
            Answer::Sent { form, outcome } => self.sent(form, outcome),
            Answer::Looked { ticket, form, typed, name, found } => self.looked(ticket, form, *typed, name, found),
        }
    }

    // ------------------------------------------------------------ forms

    /// Locks every booking with the reason (shown at the buttons and when one is tried), or
    /// lifts the lock. Reading, fetching and the sync go on.
    pub fn lock_bookings(&mut self, reason: Option<String>) {
        if reason.is_some() {
            self.close_form();
        }
        self.book_lock = reason;
    }

    /// Why bookings are locked, if they are.
    pub fn bookings_locked(&self) -> Option<&str> {
        self.book_lock.as_deref()
    }

    /// Opens a form or, on the second click, closes it again; at most one is open.
    pub fn open_form(&mut self, key: FormKey) {
        if let Some(reason) = self.book_lock.clone() {
            self.say(reason, Tone::Warn);
            return;
        }
        self.drop_pending();
        let again = self.form.as_ref() == Some(&key);
        self.close_form();
        self.message = None;
        if again {
            return;
        }
        self.form_serial += 1;
        if matches!(key, FormKey::Entry(Action::Earmark, _)) {
            // Orders and missions are asked anew each time the form opens.
            self.targets.clear();
        }
        self.prefill(&key);
        self.form = Some(key);
    }

    fn close_form(&mut self) {
        self.form = None;
        self.values = Values::default();
        self.errors.clear();
    }

    /// The values a new form starts with.
    fn prefill(&mut self, key: &FormKey) {
        let Some(data) = self.data.clone() else { return };
        let mut values = Values::default();
        match key {
            FormKey::BookIn(unit) => {
                let roster = data.rosters.get(unit).cloned().unwrap_or_default();
                let own = orgstock::own_member(&roster, self.link.as_ref().and_then(|l| l.own_account.as_deref()));
                values.holder =
                    if own.is_empty() { roster.first().map(|m| m.holder_ref.clone()).unwrap_or_default() } else { own };
            }
            FormKey::Entry(action, id) => {
                if let Some((lot, entry)) = data.entry(id) {
                    // The booking names the version the form was opened on: a
                    // change from outside meanwhile is refused, not overwritten.
                    values.version = Some(entry.version);
                    match action {
                        Action::BookOut | Action::Sell => {
                            if entry.free() > 0.0 {
                                values.amount = field_text(entry.free());
                            }
                        }
                        Action::Move => {
                            values.amount = field_text(entry.amount);
                            values.to_unit = lot.unit_id.clone();
                        }
                        Action::Note => values.note = entry.note.clone(),
                        Action::Stolen => {}
                        Action::Earmark => {
                            for job in &entry.jobs {
                                values.change.insert(format!("job:{}", job.id), field_text(job.amount));
                            }
                            for mission in &entry.missions {
                                values.change.insert(format!("mission:{}", mission.id), field_text(mission.amount));
                            }
                        }
                    }
                }
            }
        }
        self.values = values;
    }

    /// A running lookup no longer books, and the page is free again.
    fn drop_pending(&mut self) -> bool {
        let running = self.busy.is_some() && self.ticket != 0;
        if running {
            self.ticket = 0;
            self.busy = None;
            self.message = None;
        }
        running
    }

    fn error(&mut self, field: &'static str, text: String) {
        self.errors.insert(field, text);
    }

    /// Sends one change set; afterwards the feed is fetched.
    fn submit(&mut self, ctx: Option<&egui::Context>, ops: Vec<Value>) {
        if let Some(reason) = self.book_lock.clone() {
            self.say(reason, Tone::Warn);
            return;
        }
        if self.busy.is_some() {
            self.say(t("s_sle_noch_dran"), Tone::Quiet);
            return;
        }
        let Some(link) = self.link.clone() else { return };
        let (details, write) = self.data.as_ref().map_or((false, false), |d| (d.details, d.write));
        let form = self.form_serial;
        self.ticket = 0;
        self.say(t("s_sl_bucht"), Tone::Quiet);
        self.busy = Some(spawn(ctx, move || {
            let outcome = orgstock::send(&link.conn, ops);
            if let Err(error) = &outcome {
                versekit_core::errors::record("orgstock_page.send", error);
            }
            // Fetched in any case: a refusal like VERSION_CONFLICT brings the new state.
            if let Err(error) = link.store.refresh(&link.conn, details, write, (link.clock)()) {
                versekit_core::errors::record("orgstock_page.refresh", &error);
            }
            Answer::Sent { form, outcome }
        }));
    }

    fn sent(&mut self, form: u64, outcome: Result<Outcome, ApiError>) {
        match outcome {
            Err(error) => self.say(error_text(&error.code), Tone::Error),
            Ok(outcome) if outcome.held => {
                if let Some(url) = outcome.confirm_url {
                    self.confirm = Some((url, self.now() + CONFIRM_LIFETIME));
                }
                self.say(t("s_sle_masse"), Tone::Warn);
            }
            Ok(outcome) if !outcome.failed.is_empty() => {
                let reasons = outcome.reasons();
                if reasons.iter().any(|r| r == "VERSION_CONFLICT") && form == self.form_serial {
                    // The new state is fetched: the next try names its version.
                    self.values.version = None;
                }
                let text: Vec<String> = reasons.iter().map(|r| reason_text(r)).collect();
                self.say(text.join(" "), Tone::Error);
            }
            Ok(outcome) => {
                let mut text = t("s_sl_gebucht");
                if outcome.offers > 0 {
                    text = format!("{text} {}", tf("s_sl_angebote", &[("0", Arg::Int(outcome.offers))]));
                }
                if form == self.form_serial {
                    self.close_form();
                }
                self.message = None;
                self.tell(text, "check");
            }
        }
    }

    /// Booking in: checks, looks the goods up, then sends.
    pub fn book_in(&mut self, ctx: Option<&egui::Context>) {
        let Some(FormKey::BookIn(unit)) = self.form.clone() else { return };
        let Some(data) = self.data.clone() else { return };
        self.errors.clear();
        let values = self.values.clone();
        let name = values.material.trim().to_string();
        let place = values.location.trim().to_string();
        let mut wrong = false;
        if values.holder.is_empty() {
            self.error("holder", t("s_sl_besitzer_fehlt"));
            wrong = true;
        }
        if name.is_empty() {
            self.error("material", t("s_sl_ware_fehlt"));
            wrong = true;
        }
        if place.is_empty() {
            self.error("location", t("s_sl_ort_fehlt"));
            wrong = true;
        }
        let piece = values.piece;
        let amount = match orgstock::amount_ok(orgstock::number(&values.amount), piece, None, false) {
            Ok(a) => Some(a),
            Err(e) => {
                self.error("amount", amount_error_text(e, piece));
                None
            }
        };
        // Pieces have no quality field: an old value does not count.
        let quality = if piece { Some(0.0) } else { Some(orgstock::number(&values.quality).unwrap_or(0.0)) };
        let quality = quality.filter(|q| (0.0..=1000.0).contains(q) && *q == q.trunc());
        if quality.is_none() {
            self.error("quality", t("s_sl_qualitaet_falsch"));
        }
        let (Some(amount), Some(quality), false) = (amount, quality, wrong) else { return };
        let book = BookIn { unit, holder: values.holder.clone(), place, quality: quality as i64, amount, piece };
        if let Some(chosen) = &values.material_ref
            && lower(&chosen.name) == lower(&name)
        {
            let chosen = chosen.clone();
            return self.send_book_in(ctx, &book, &chosen);
        }
        if let Some(known) = data.known_material(&name, if piece { PIECE } else { SCU }) {
            return self.send_book_in(ctx, &book, &known);
        }
        self.look_up(ctx, name, values);
    }

    fn send_book_in(&mut self, ctx: Option<&egui::Context>, book: &BookIn, found: &Found) {
        // An item counts in pieces; in SCU the Basetool would refuse it.
        if found.kind == orgstock::ITEM && !book.piece {
            self.say(tf("s_sle_ware_stueck", &[("0", Arg::from(found.name.as_str()))]), Tone::Error);
            return;
        }
        let v = &self.values;
        // Only the id: a name beside it gives vague matches at the Basetool.
        let op = orgstock::op_book_in(
            &book.unit,
            &book.holder,
            Value::Object(vec![("bt".into(), Value::str(&found.bt))]),
            Value::Object(vec![("name".into(), Value::str(&book.place))]),
            book.quality,
            v.stolen,
            book.amount,
            if book.piece { PIECE } else { SCU },
            &v.note,
            v.merge,
        );
        self.submit(ctx, vec![op]);
    }

    /// The reference of the goods list for the lookup (`stock_catalog.reference`).
    fn reference(&self, name: &str, piece: bool) -> Option<(&'static str, Value)> {
        let entry = self.catalog.choices().get(&lower(name.trim()))?;
        let use_item = entry.item.is_some() && (entry.material.is_none() || piece);
        let number = if use_item { entry.item.clone() } else { entry.material.clone() }?;
        let shown: String = entry.name.chars().take(200).collect();
        Some((
            if use_item { orgstock::ITEM } else { orgstock::MATERIAL },
            Value::Object(vec![("uexId".into(), number), ("name".into(), Value::str(shown))]),
        ))
    }

    fn look_up(&mut self, ctx: Option<&egui::Context>, name: String, typed: Values) {
        if self.busy.is_some() {
            self.say(t("s_sle_noch_dran"), Tone::Quiet);
            return;
        }
        let Some(link) = self.link.clone() else { return };
        let Some(form) = self.form.clone() else { return };
        self.ticket += 1;
        let ticket = self.ticket;
        let reference = self.reference(&name, typed.piece);
        let piece = typed.piece;
        self.say(t("s_sl_ware_pruefen"), Tone::Quiet);
        self.busy = Some(spawn(ctx, move || {
            let known = reference.as_ref().map(|(kind, r)| (*kind, r));
            let found = orgstock::resolve_material(&link.conn, &name, known, piece);
            Answer::Looked { ticket, form, typed: Box::new(typed), name, found }
        }));
    }

    /// The answer of a lookup books only while its ticket holds, the same
    /// form is open, booking is still allowed and the form stands as it did.
    fn looked(&mut self, ticket: u64, form: FormKey, typed: Values, name: String, found: Result<Resolved, ApiError>) {
        if ticket != self.ticket || ticket == 0 {
            return;
        }
        self.ticket = 0;
        self.message = None;
        if self.form.as_ref() != Some(&form) {
            return;
        }
        let FormKey::BookIn(unit) = &form else { return };
        if !self.data.as_ref().is_some_and(|d| d.can_write(unit)) {
            self.say(t("s_sl_nicht_mehr"), Tone::Error);
            return;
        }
        if self.values.typed() != typed.typed() {
            self.say(t("s_sle_ware_geaendert"), Tone::Warn);
            return;
        }
        match found {
            Err(error) => {
                versekit_core::errors::record("orgstock_page.resolve", &error);
                self.say(error_text(&error.code), Tone::Error);
            }
            Ok(Resolved::One(hit)) => {
                self.values.material_ref = Some(hit.clone());
                self.values.candidates.clear();
                let book = BookIn {
                    unit: unit.clone(),
                    holder: typed.holder.clone(),
                    place: typed.location.trim().to_string(),
                    quality: if typed.piece { 0 } else { orgstock::number(&typed.quality).unwrap_or(0.0) as i64 },
                    amount: orgstock::number(&typed.amount).unwrap_or(0.0),
                    piece: typed.piece,
                };
                self.send_book_in(None, &book, &hit);
            }
            Ok(Resolved::Several(hits)) => {
                self.values.candidates = hits;
                self.values.candidates_for = lower(name.trim());
                self.say(tf("s_sle_ware_mehrdeutig", &[("0", Arg::from(name))]), Tone::Warn);
            }
            Ok(Resolved::Unmatched) => {
                self.values.candidates.clear();
                self.say(tf("s_sle_ware_unbekannt", &[("0", Arg::from(name))]), Tone::Error);
            }
        }
    }

    /// A hit of the lookup chosen: it is booked with the next click.
    pub fn choose_candidate(&mut self, hit: Found) {
        self.drop_pending();
        self.values.material = hit.name.clone();
        self.values.material_ref = Some(hit);
        self.values.candidates.clear();
        self.message = None;
    }

    /// The reservation amounts of the form.
    fn earmarks(&self, entry: &Entry) -> (Earmarks, Earmarks) {
        let mut jobs = Earmarks::new();
        let mut missions = Earmarks::new();
        for job in &entry.jobs {
            if let Some(a) = self.values.from.get(&format!("job:{}", job.id)).and_then(|t| orgstock::number(t))
                && a != 0.0
            {
                jobs.insert(job.id.clone(), a);
            }
        }
        for mission in &entry.missions {
            if let Some(a) = self.values.from.get(&format!("mission:{}", mission.id)).and_then(|t| orgstock::number(t))
                && a != 0.0
            {
                missions.insert(mission.id.clone(), a);
            }
        }
        (jobs, missions)
    }

    /// The booking of the open entry form; `extra` names a button of the
    /// earmark form (`change:<id>`, `remove:<id>`, `add`) or the note form
    /// (`clear`).
    pub fn book_entry(&mut self, ctx: Option<&egui::Context>, extra: Option<&str>) {
        let Some(FormKey::Entry(action, id)) = self.form.clone() else { return };
        let Some(data) = self.data.clone() else { return };
        let Some((lot, current)) = data.entry(&id) else {
            self.close_form();
            self.say(t("s_sle_eintrag_weg"), Tone::Warn);
            return;
        };
        let entry = &Entry { version: self.values.version.unwrap_or(current.version), ..current.clone() };
        self.errors.clear();
        let piece = lot.piece();
        let unit = lot.unit_id.as_str();
        let amount_of =
            |state: &mut OrgStockState, text: &str, optional: bool, field: &'static str| match orgstock::amount_ok(
                orgstock::number(text),
                piece,
                Some(entry.amount),
                optional,
            ) {
                Ok(a) => Some(a),
                Err(e) => {
                    state.error(field, amount_error_text(e, piece));
                    None
                }
            };
        let op = match action {
            Action::BookOut | Action::Sell => {
                let amount = amount_of(self, &self.values.amount.clone(), false, "amount");
                let (jobs, missions) = self.earmarks(entry);
                if action == Action::Sell {
                    let terminal = self.values.terminal.trim().to_string();
                    let proceeds = orgstock::number(&self.values.proceeds).filter(|p| p.is_finite() && *p >= 0.0);
                    if terminal.is_empty() {
                        self.error("terminal", t("s_sl_verkauf_fehlt"));
                    }
                    if proceeds.is_none() {
                        self.error("proceeds", t("s_sl_verkauf_fehlt"));
                    }
                    let (Some(amount), Some(proceeds), false) = (amount, proceeds, terminal.is_empty()) else { return };
                    orgstock::op_sell(unit, entry, amount, &lot.unit_q, &terminal, proceeds, &jobs, &missions)
                } else {
                    let Some(amount) = amount else { return };
                    orgstock::op_book_out(unit, entry, amount, &lot.unit_q, &jobs, &missions)
                }
            }
            Action::Move => match self.values.mode {
                MoveMode::Pool => {
                    let targets: Vec<String> =
                        data.writable_units().iter().filter(|u| u.id != lot.unit_id).map(|u| u.id.clone()).collect();
                    let amount = amount_of(self, &self.values.amount.clone(), true, "amount");
                    if targets.is_empty() {
                        self.error("to_unit", t("s_sle_keine_zieleinheit"));
                        return;
                    }
                    if !targets.contains(&self.values.to_unit) {
                        self.error("to_unit", t("s_sle_ziel_fehlt"));
                        return;
                    }
                    let Some(amount) = amount else { return };
                    orgstock::op_move_pool(
                        unit,
                        entry,
                        &self.values.to_unit,
                        (amount > 0.0).then_some(amount),
                        &lot.unit_q,
                        self.values.merge,
                    )
                }
                MoveMode::Holder => {
                    let amount = amount_of(self, &self.values.amount.clone(), false, "amount");
                    let to_unit =
                        if self.values.to_unit.is_empty() { lot.unit_id.clone() } else { self.values.to_unit.clone() };
                    let roster = data.rosters.get(&to_unit).cloned().unwrap_or_default();
                    if !roster.iter().any(|m| m.holder_ref == self.values.holder) {
                        self.error("holder", t("s_sl_besitzer_fehlt"));
                        return;
                    }
                    let Some(amount) = amount else { return };
                    let (jobs, missions) = self.earmarks(entry);
                    orgstock::op_move_holder(
                        unit,
                        entry,
                        amount,
                        &lot.unit_q,
                        &to_unit,
                        &self.values.holder,
                        &jobs,
                        &missions,
                        self.values.merge,
                    )
                }
                MoveMode::Location => {
                    let amount = amount_of(self, &self.values.amount.clone(), false, "amount");
                    let place = self.values.location.trim().to_string();
                    if place.is_empty() {
                        self.error("location", t("s_sl_ort_fehlt"));
                    }
                    let (Some(amount), false) = (amount, place.is_empty()) else { return };
                    let (jobs, missions) = self.earmarks(entry);
                    orgstock::op_move_location(
                        unit,
                        entry,
                        amount,
                        &lot.unit_q,
                        Value::Object(vec![("name".into(), Value::str(place))]),
                        &jobs,
                        &missions,
                        self.values.merge,
                    )
                }
            },
            Action::Note => {
                let note = if extra == Some("clear") { String::new() } else { self.values.note.clone() };
                orgstock::op_note(unit, entry, &note)
            }
            Action::Stolen => {
                let Some(amount) = amount_of(self, &self.values.amount.clone(), true, "amount") else { return };
                orgstock::op_stolen(unit, entry, !lot.stolen, (amount > 0.0).then_some(amount), &lot.unit_q)
            }
            Action::Earmark => {
                let Some(extra) = extra else { return };
                // `job:<id>` or `mission:<id>`: the ids of both kinds may be alike.
                let target_of = |line: &str| match line.split_once(':') {
                    Some(("job", id)) => Some(("jobOrder", id.to_string())),
                    Some(("mission", id)) => Some(("mission", id.to_string())),
                    _ => None,
                };
                if let Some((kind, target)) = extra.strip_prefix("remove:").and_then(target_of) {
                    orgstock::op_allocation("remove", unit, entry, kind, &target, 0.0, &lot.unit_q)
                } else if let Some(line) = extra.strip_prefix("change:") {
                    let Some((kind, target)) = target_of(line) else { return };
                    let text = self.values.change.get(line).cloned().unwrap_or_default();
                    let Some(amount) = amount_of(self, &text, false, "change") else { return };
                    orgstock::op_allocation("change", unit, entry, kind, &target, amount, &lot.unit_q)
                } else {
                    let amount = amount_of(self, &self.values.new_amount.clone(), false, "new_amount");
                    let choice = self.values.new_target.clone();
                    let Some((kind, target)) = choice.split_once(':') else {
                        self.error("new_target", t("s_sle_ziel_fehlt"));
                        return;
                    };
                    let Some(amount) = amount else { return };
                    let kind = if kind == "job" { "jobOrder" } else { "mission" };
                    orgstock::op_allocation("add", unit, entry, kind, target, amount, &lot.unit_q)
                }
            }
        };
        self.submit(ctx, vec![op]);
    }

    /// The storage locations of the Basetool, fetched once an hour.
    fn locations(&mut self, ctx: Option<&egui::Context>) -> Vec<String> {
        let now = self.now();
        let stale = self.locations.as_ref().is_none_or(|(_, at)| now - at > LOCATIONS_AGAIN);
        if stale
            && self.locations_loading.is_none()
            && let Some(link) = self.link.clone()
        {
            self.locations_loading = Some(spawn(ctx, move || {
                let mut cache = Locations::default();
                let places = cache.get(&link.conn, (link.clock)())?;
                let mut names: Vec<String> = places
                    .iter()
                    .filter_map(|(_, v)| v.get("name").and_then(Value::as_str))
                    .map(str::to_string)
                    .collect();
                names.sort_by_key(|n| lower(n));
                Ok(names)
            }));
        }
        self.locations.as_ref().map(|(names, _)| names.clone()).unwrap_or_default()
    }

    /// Orders and missions of a pool for these goods, fetched once per page.
    fn targets_for(&mut self, ctx: Option<&egui::Context>, unit: &str, bt: &str) -> Option<Targets> {
        let key = (unit.to_string(), bt.to_string());
        if let Some(found) = self.targets.get(&key) {
            return found.clone();
        }
        let link = self.link.clone()?;
        self.targets.insert(key.clone(), None);
        let (unit, bt) = key.clone();
        self.targets_loading.push((key, spawn(ctx, move || orgstock::targets(&link.conn, &unit, &bt))));
        None
    }

    /// The names the goods field offers: the goods list and the goods in a
    /// pool already, each once, sorted.
    fn material_names(&mut self) -> Arc<Vec<String>> {
        if let Some((seen, names)) = &self.names
            && *seen == self.seen
        {
            return names.clone();
        }
        let mut names: BTreeMap<String, String> =
            self.catalog.choices().values().map(|c| (lower(&c.name), c.name.clone())).collect();
        if let Some(data) = &self.data {
            for lot in &data.lots {
                if !lot.name.is_empty() {
                    names.entry(lower(&lot.name)).or_insert_with(|| lot.name.clone());
                }
            }
        }
        let mut out: Vec<String> = names.into_values().collect();
        out.sort_by_key(|n| lower(n));
        let out = Arc::new(out);
        // Built once per state of the storage, not in every frame.
        self.names = Some((self.seen, out.clone()));
        out
    }

    /// The kind of a lot; for materials with the Basetool's `materialKind`.
    pub fn kind_of(&self, lot: &Lot) -> Kind {
        if !lot.is_item() {
            return self.catalog.material_kind_with(&lot.name, lot.kind.as_ref());
        }
        let kind = self.catalog.item_kind(&lot.name);
        match &lot.component {
            Some(c) if kind == Kind::Other && !c.kind.is_empty() => {
                if WEAPON_COMPONENTS.contains(&c.kind.as_str()) {
                    Kind::VehicleWeapon
                } else {
                    Kind::VehicleItem
                }
            }
            _ => kind,
        }
    }

    /// The lots that tab, kind, place and search let through.
    pub fn visible_lots<'a>(&self, data: &'a OrgStockData) -> Vec<&'a Lot> {
        data.lots_of(&self.unit, &self.search)
            .into_iter()
            .filter(|lot| lot.is_item() == self.items)
            .filter(|lot| self.kind.is_none_or(|k| self.kind_of(lot) == k))
            .filter(|lot| self.place.is_empty() || lower(&lot.place) == lower(&self.place))
            .collect()
    }

    /// Picks a unit: the forms of the old one end, all lots fold.
    pub fn choose_unit(&mut self, id: &str) {
        if id == self.unit {
            return;
        }
        self.drop_pending();
        self.unit = id.to_string();
        self.unit_chosen = true;
        self.open.clear();
        self.close_form();
        self.message = None;
    }
}

/// What booking in sends besides the goods.
struct BookIn {
    unit: String,
    holder: String,
    place: String,
    quality: i64,
    amount: f64,
    piece: bool,
}

// ------------------------------------------------------------ drawing

fn small(ui: &mut Ui, scheme: &Scheme, text: &str, tone: Color) {
    let _ = scheme;
    widgets::reading(ui, RichText::new(text).small().color(color(tone)));
}

fn tone_color(scheme: &Scheme, tone: Tone) -> Color {
    match tone {
        Tone::Quiet => scheme.sub,
        Tone::Done => scheme.accent,
        Tone::Warn => scheme.yellow,
        Tone::Error => scheme.red_text(),
    }
}

fn named_icon(ui: &mut Ui, scheme: &Scheme, icon: &str, meaning: &str, on: bool) -> egui::Response {
    let response = if on {
        let (rect, response) =
            ui.allocate_exact_size(vec2(widgets::ICON_BUTTON_SIZE, widgets::ICON_BUTTON_SIZE), Sense::click());
        let colors = widgets::button_colors(scheme, ButtonKind::Strong, response.hovered());
        ui.painter().rect_filled(rect.shrink(1.0), crate::control_radius(scheme), color(colors.fill));
        if let Some(image) = crate::icons::image(icon, colors.text, 16.0) {
            image.paint_at(ui, Rect::from_center_size(rect.center(), vec2(16.0, 16.0)));
        }
        response.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(meaning)
    } else {
        widgets::icon_button(ui, scheme, icon, meaning)
    };
    response.widget_info(|| WidgetInfo::selected(Role::Button, true, on, meaning));
    response
}

fn field_label(ui: &mut Ui, text: &str) {
    ui.add_space(6.0);
    ui.label(RichText::new(text).strong());
}

fn field_error(ui: &mut Ui, scheme: &Scheme, error: Option<&String>) {
    if let Some(error) = error {
        ui.add(egui::Label::new(RichText::new(error).color(color(scheme.red_text()))).wrap());
    }
}

/// A plain field; its error goes as soon as the input changes. Returns
/// whether Enter was pressed in it.
fn text_field(
    ui: &mut Ui,
    scheme: &Scheme,
    text: &mut String,
    hint: &str,
    errors: &mut HashMap<&'static str, String>,
    field: &'static str,
) -> bool {
    let width = ui.available_width();
    let error = errors.get(field).cloned();
    // Its own id: the field takes its id from the hint, and several fields share one.
    let response =
        ui.push_id(field, |ui| widgets::checked_field(ui, scheme, text, hint, width, error.as_deref())).inner;
    let name = field_name(field);
    let shown = text.clone();
    response.widget_info(|| {
        let mut info = WidgetInfo::text_edit(true, "", &shown, hint);
        info.label = Some(name.clone());
        info
    });
    if response.changed() {
        errors.remove(field);
    }
    response.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter))
}

/// What a screen reader names a field.
fn field_name(field: &str) -> String {
    t(match field {
        "quality" => "s_sl_qualitaet",
        "note" => "s_sl_notiz",
        "terminal" => "s_sl_terminal",
        "proceeds" => "s_sl_erloes",
        _ => "s_sl_menge",
    })
}

/// A field with suggestions; red while its input is wrong.
#[allow(clippy::too_many_arguments)]
fn suggest_field(
    ui: &mut Ui,
    scheme: &Scheme,
    salt: &str,
    hint: &str,
    text: &mut String,
    entries: &[String],
    errors: &mut HashMap<&'static str, String>,
    field: &'static str,
) -> (bool, bool) {
    let more = |n: usize| tf("s_mlg_mehr", &[("n", Arg::from(n))]);
    let error = errors.get(field).cloned();
    let out = ui
        .scope(|ui| {
            if error.is_some() {
                let red = egui::Stroke::new(1.0, color(scheme.red));
                let w = &mut ui.visuals_mut().widgets;
                (w.inactive.bg_stroke, w.hovered.bg_stroke) = (red, red);
            }
            widgets::SuggestField::new(salt, hint, scheme, &more).width(ui.available_width()).show(ui, text, entries)
        })
        .inner;
    if out.response.changed() {
        errors.remove(field);
    }
    field_error(ui, scheme, error.as_ref());
    let enter = out.picked.is_none() && out.response.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
    (enter, out.picked.is_some())
}

/// A choice from a few entries; `chosen` is the id.
fn choice(
    ui: &mut Ui,
    scheme: &Scheme,
    salt: &str,
    name: &str,
    entries: &[(String, String)],
    chosen: &mut String,
    error: Option<&String>,
) -> bool {
    let shown = entries.iter().find(|(id, _)| id == chosen).map_or(String::new(), |(_, label)| label.clone());
    let before = chosen.clone();
    let combo = egui::ComboBox::from_id_salt(salt)
        .selected_text(shown.as_str())
        .width(ui.available_width().min(320.0))
        .show_ui(ui, |list| {
            for (id, label) in entries {
                list.selectable_value(chosen, id.clone(), label.as_str());
            }
        });
    widgets::name_combo(&combo.response, name, &shown);
    field_error(ui, scheme, error);
    *chosen != before
}

/// The unit storage page.
pub fn show_page(ui: &mut Ui, scheme: &Scheme, state: &mut OrgStockState) {
    let ctx = ui.ctx().clone();
    state.tick(Some(&ctx));
    small(ui, scheme, &t("s_sl_lead"), scheme.sub);
    if state.link.as_ref().is_some_and(|l| l.demo) {
        ui.horizontal_wrapped(|row| {
            if let Some(image) = crate::icons::themed("info", scheme, IconTint::Highlight, 14.0) {
                row.add(image);
            }
            small(row, scheme, &t("s_sle_beispiel"), scheme.sub);
        });
    }
    if let Some(error) = state.fetch_error.clone() {
        small(ui, scheme, &error, scheme.red_text());
    }
    ui.add_space(4.0);
    match state.data.clone() {
        None => waiting(ui, scheme, state),
        Some(data) if data.units.is_empty() => {
            widgets::empty_state(ui, scheme, "users", &t("s_sl_keine_einheit"));
        }
        Some(data) => body(ui, scheme, state, &data),
    }
    if let Some((text, since)) = &mut state.notice {
        let now = ui.input(|i| i.time);
        let at = *since.get_or_insert(now);
        if !widgets::notice(ui.ctx(), scheme, at, state.notice_icon, text, None).shown {
            state.notice = None;
        }
    }
}

fn waiting(ui: &mut Ui, scheme: &Scheme, state: &mut OrgStockState) {
    ui.label(t("s_sl_warten"));
    ui.add_space(6.0);
    if state.fetching.is_some()
        || state.last_fetch_asked.is_some_and(|_| state.link.as_ref().is_some_and(|l| l.sync.is_some()))
    {
        widgets::loading(ui, scheme, &t("s_bt_laeuft"));
    }
    if state.link.is_some()
        && ui.add(widgets::Button::new(&t("s_bt_jetzt"), ButtonKind::Normal, scheme).icon("rotate-cw")).clicked()
    {
        let ctx = ui.ctx().clone();
        state.sync_now(Some(&ctx));
    }
}

fn role_hint(data: &OrgStockData, unit: &str) -> Option<(&'static str, bool)> {
    let logistician = data.logistician.contains(unit);
    if logistician && !data.details {
        Some(("s_sle_details_fehlt", true))
    } else if logistician && !data.write {
        Some(("s_sle_buchen_aus", false))
    } else if !logistician {
        Some((if data.write { "s_sle_kein_logistiker" } else { "s_sle_mitglied" }, false))
    } else {
        None
    }
}

fn body(ui: &mut Ui, scheme: &Scheme, state: &mut OrgStockState, data: &Arc<OrgStockData>) {
    let ctx = ui.ctx().clone();
    // The unit.
    if data.units.len() > 1 {
        let entries: Vec<(String, String)> = data.units.iter().map(|u| (u.id.clone(), u.label())).collect();
        let mut chosen = state.unit.clone();
        ui.horizontal_wrapped(|row| {
            row.label(RichText::new(t("s_sl_ziel_einheit")).color(color(scheme.sub)));
            if choice(row, scheme, "lager-einheit-wahl", &t("s_sl_ziel_einheit"), &entries, &mut chosen, None) {
                state.choose_unit(&chosen);
            }
        });
    } else {
        ui.label(RichText::new(data.units[0].label()).strong());
    }
    let unit = state.unit.clone();
    if let Some((key, warn)) = role_hint(data, &unit) {
        small(ui, scheme, &t(key), if warn { scheme.yellow } else { scheme.sub });
    }
    // Locked from outside (the real Basetool in a test environment): said above the list.
    if let Some(reason) = state.bookings_locked() {
        small(ui, scheme, reason, scheme.yellow);
    }
    ui.add_space(GAP);

    // Tabs with counts and the page's actions.
    let every = data.lots_of(&unit, "");
    let items = every.iter().filter(|l| l.is_item()).count();
    let materials = every.len() - items;
    let tabs = [format!("{} ({materials})", t("s_ml_material")), format!("{} ({items})", t("s_ml_items"))];
    let writable = data.can_write(&unit);
    let book_word = t("s_sl_einbuchen");
    let sync_word = t("s_bt_jetzt");
    let booking_open = matches!(&state.form, Some(FormKey::BookIn(_)));
    let mut actions = Vec::new();
    if writable {
        actions.push(widgets::PageAction { icon: "package-plus", text: &book_word, on: booking_open });
    }
    actions.push(widgets::PageAction { icon: "rotate-cw", text: &sync_word, on: false });
    let mut chosen = usize::from(state.items);
    let (changed, clicked) =
        widgets::tab_bar_with_actions(ui, scheme, &[tabs[0].as_str(), tabs[1].as_str()], &mut chosen, &actions);
    if changed {
        state.items = chosen == 1;
        state.kind = None;
    }
    match clicked.map(|i| if writable { i } else { i + 1 }) {
        Some(0) => state.open_form(FormKey::BookIn(unit.clone())),
        Some(1) => state.sync_now(Some(&ctx)),
        _ => {}
    }
    ui.add_space(4.0);

    // Kind and place.
    let here: Vec<&Lot> = every.iter().copied().filter(|l| l.is_item() == state.items).collect();
    let present: BTreeSet<Kind> = here.iter().map(|l| state.kind_of(l)).collect();
    let kinds: Vec<Kind> = (if state.items { &ITEM_KINDS[..] } else { &MATERIAL_KINDS[..] })
        .iter()
        .copied()
        .filter(|k| present.contains(k))
        .collect();
    if state.kind.is_some_and(|k| !kinds.contains(&k)) {
        state.kind = None;
    }
    let mut places: Vec<String> = Vec::new();
    for lot in &here {
        if !lot.place.is_empty() && !places.contains(&lot.place) {
            places.push(lot.place.clone());
        }
    }
    places.sort_by_key(|p| lower(p));
    if !state.place.is_empty() && !places.contains(&state.place) {
        state.place.clear();
    }
    let kind_words: Vec<String> =
        std::iter::once(t("s_ml_alle_arten")).chain(kinds.iter().map(|k| t(&k.text_key()))).collect();
    let place_words: Vec<String> = std::iter::once(t("s_ml_alle_orte")).chain(places.iter().cloned()).collect();
    let kind_refs: Vec<&str> = kind_words.iter().map(String::as_str).collect();
    let place_refs: Vec<&str> = place_words.iter().map(String::as_str).collect();
    let mut picked = vec![
        state.kind.and_then(|k| kinds.iter().position(|x| *x == k)).map_or(0, |p| p + 1),
        places.iter().position(|p| *p == state.place).map_or(0, |p| p + 1),
    ];
    if widgets::filter_bar(ui, "org-lager", &[&kind_refs, &place_refs], &mut picked) {
        state.kind = picked[0].checked_sub(1).and_then(|i| kinds.get(i).copied());
        state.place = picked[1].checked_sub(1).and_then(|i| places.get(i).cloned()).unwrap_or_default();
    }
    ui.add_space(4.0);
    let width = ui.available_width();
    widgets::search_field(ui, &mut state.search, &t("s_sl_suche"), scheme, width);

    // Message, confirmation, booking in.
    // The form shows its own message, but only where it is drawn: folded
    // away or filtered out, the message stands up here.
    let form_shown = match &state.form {
        None => false,
        Some(FormKey::BookIn(_)) => true,
        Some(FormKey::Entry(_, id)) => {
            state.visible_lots(data).iter().any(|lot| state.open.contains(&lot.key) && lot.entry(id).is_some())
        }
    };
    if !form_shown && let Some((text, tone)) = state.message.clone() {
        ui.add_space(4.0);
        ui.horizontal_wrapped(|row| {
            if state.busy.is_some() {
                row.add(egui::Spinner::new().color(color(scheme.accent)));
            }
            small(row, scheme, &text, tone_color(scheme, tone));
        });
    }
    let now = state.now();
    if state.confirm.as_ref().is_some_and(|(_, until)| now >= *until) {
        state.confirm = None;
    }
    if let Some((url, _)) = state.confirm.clone() {
        ui.add_space(GAP);
        widgets::card(ui, scheme, |card| {
            card.set_width(card.available_width());
            widgets::card_title(card, scheme, &t("s_bt_masse"));
            small(card, scheme, &t("s_sl_masse_h"), scheme.sub);
            card.add_space(4.0);
            if card
                .add(widgets::Button::new(&t("s_bt_masse_oeffnen"), ButtonKind::Strong, scheme).icon("external-link"))
                .clicked()
            {
                if state.link.as_ref().is_some_and(|l| l.demo) {
                    state.tell(t("s_sle_beispiel_browser"), "check");
                } else {
                    versekit_core::browser::open_in_browser(&url);
                }
            }
        });
    }
    if let Some(FormKey::BookIn(form_unit)) = state.form.clone() {
        ui.add_space(GAP);
        widgets::card(ui, scheme, |card| {
            card.set_width(card.available_width());
            book_in_form(card, scheme, state, data, &form_unit);
        });
    }
    ui.add_space(GAP);
    list(ui, scheme, state, data);
}

fn form_message(ui: &mut Ui, scheme: &Scheme, state: &OrgStockState) {
    if let Some((text, tone)) = &state.message {
        ui.add_space(4.0);
        ui.horizontal_wrapped(|row| {
            if state.busy.is_some() {
                row.add(egui::Spinner::new().color(color(scheme.accent)));
            }
            small(row, scheme, text, tone_color(scheme, *tone));
        });
    }
}

fn form_buttons(ui: &mut Ui, scheme: &Scheme, go: &str) -> (bool, bool) {
    ui.add_space(GAP);
    let mut out = (false, false);
    ui.horizontal_wrapped(|row| {
        out.0 = row.add(widgets::Button::new(go, ButtonKind::Strong, scheme)).clicked();
        out.1 = row.add(widgets::Button::new(&t("s_bt_abbrechen"), ButtonKind::Normal, scheme)).clicked();
    });
    out
}

fn book_in_form(ui: &mut Ui, scheme: &Scheme, state: &mut OrgStockState, data: &OrgStockData, unit: &str) {
    let ctx = ui.ctx().clone();
    ui.label(RichText::new(t("s_sl_einbuchen_titel")).strong());
    let mut enter = false;
    let roster = data.rosters.get(unit).cloned().unwrap_or_default();
    field_label(ui, &t("s_sl_besitzer"));
    if roster.is_empty() {
        small(ui, scheme, &t("s_sl_keine_mitglieder"), scheme.yellow);
        field_error(ui, scheme, state.errors.get("holder"));
    } else {
        let entries: Vec<(String, String)> = roster.iter().map(|m| (m.holder_ref.clone(), m.name.clone())).collect();
        let error = state.errors.get("holder").cloned();
        if choice(
            ui,
            scheme,
            "lager-einbuchen-besitzer",
            &t("s_sl_besitzer"),
            &entries,
            &mut state.values.holder,
            error.as_ref(),
        ) {
            state.errors.remove("holder");
        }
    }
    field_label(ui, &t("s_sl_ware"));
    let names = state.material_names();
    let before = state.values.material.clone();
    let (hit_enter, picked) = suggest_field(
        ui,
        scheme,
        "lager-einbuchen-ware",
        &t("s_sl_ware_h"),
        &mut state.values.material,
        &names,
        &mut state.errors,
        "material",
    );
    enter |= hit_enter;
    if state.values.material != before {
        if lower(state.values.material.trim()) != state.values.candidates_for {
            state.values.candidates.clear();
        }
        state.values.material_ref = None;
        state.drop_pending();
    }
    let typed = lower(state.values.material.trim());
    if let Some(entry) = state.catalog.choices().get(&typed).cloned() {
        // What the goods list knows: goods, section or both.
        let mut parts = Vec::new();
        if entry.material.is_some() {
            parts.push(t("s_sl_k_ware"));
        }
        if entry.item.is_some() {
            parts.push(t(section_key(&entry.section)));
        }
        small(ui, scheme, &parts.join(" · "), scheme.sub);
        // A picked name sets the unit: an item in pieces, goods in SCU.
        if picked && (entry.item.is_none() || entry.material.is_none()) {
            state.values.piece = entry.material.is_none();
        }
    }
    if !state.values.candidates.is_empty() {
        small(ui, scheme, &t("s_sl_ware_vorschlaege"), scheme.yellow);
        let hits = state.values.candidates.clone();
        for hit in hits {
            let kind = t(if hit.kind == orgstock::MATERIAL { "s_sl_k_ware" } else { "s_sl_k_gegenstand" });
            let text = format!("{}  ·  {kind}", hit.name);
            if widgets::list_row(ui, scheme, false, &text).clicked() {
                state.choose_candidate(hit);
            }
        }
    }
    field_label(ui, &t("s_sl_ort"));
    let places = state.locations(Some(&ctx));
    let (hit_enter, _) = suggest_field(
        ui,
        scheme,
        "lager-einbuchen-ort",
        &t("s_sl_ort_h"),
        &mut state.values.location,
        &places,
        &mut state.errors,
        "location",
    );
    enter |= hit_enter;
    if state.locations_loading.is_some() {
        small(ui, scheme, &t("s_sl_orte_laden"), scheme.sub);
    }
    field_label(ui, &t("s_sl_einheit_menge"));
    ui.horizontal_wrapped(|row| {
        for (piece, word) in [(false, "SCU".to_string()), (true, t("s_lg_stueck"))] {
            if widgets::radio(row, scheme, state.values.piece == piece, &word).clicked() && state.values.piece != piece
            {
                state.values.piece = piece;
                state.values.material_ref = None;
                state.drop_pending();
            }
        }
    });
    crate::layout::columns_or_stack(ui, 200.0, 2, |col, i| {
        if i == 0 {
            field_label(col, &t("s_sl_menge"));
            enter |= text_field(col, scheme, &mut state.values.amount, "", &mut state.errors, "amount");
        } else if !state.values.piece {
            field_label(col, &t("s_sl_qualitaet"));
            enter |= text_field(col, scheme, &mut state.values.quality, "0", &mut state.errors, "quality");
        }
    });
    ui.add_space(4.0);
    if !state.values.piece {
        widgets::switch_row(ui, scheme, &t("s_sl_zusammenlegen"), &mut state.values.merge);
    }
    widgets::switch_row(ui, scheme, &t("s_sl_gestohlen_feld"), &mut state.values.stolen);
    field_label(ui, &t("s_sl_notiz"));
    enter |= text_field(ui, scheme, &mut state.values.note, "", &mut state.errors, "note");
    form_message(ui, scheme, state);
    let (go, cancel) = form_buttons(ui, scheme, &t("s_sl_einbuchen"));
    if go || enter {
        state.book_in(Some(&ctx));
    } else if cancel {
        state.drop_pending();
        state.close_form();
        state.message = None;
    }
}

/// The amount fields "of it from #12" for each reservation of an entry.
fn earmark_fields(ui: &mut Ui, scheme: &Scheme, state: &mut OrgStockState, entry: &Entry, piece: bool) {
    let lines: Vec<(String, String, f64)> = entry
        .jobs
        .iter()
        .map(|j| (format!("job:{}", j.id), job_label(j.number), j.amount))
        .chain(entry.missions.iter().map(|m| {
            (format!("mission:{}", m.id), if m.name.is_empty() { "?".into() } else { m.name.clone() }, m.amount)
        }))
        .collect();
    for (key, label, amount) in lines {
        let name = tf("s_sl_davon_aus", &[("0", Arg::from(label))]);
        field_label(ui, &name);
        let width = ui.available_width();
        let hint = amount_text(amount, piece);
        let text = state.values.from.entry(key.clone()).or_default();
        let response = ui.push_id(&key, |ui| widgets::checked_field(ui, scheme, text, &hint, width, None)).inner;
        let shown = text.clone();
        response.widget_info(|| {
            let mut info = WidgetInfo::text_edit(true, "", &shown, &hint);
            info.label = Some(name.clone());
            info
        });
    }
}

fn entry_form(
    ui: &mut Ui,
    scheme: &Scheme,
    state: &mut OrgStockState,
    data: &OrgStockData,
    action: Action,
    lot: &Lot,
    entry: &Entry,
) {
    let ctx = ui.ctx().clone();
    let piece = lot.piece();
    let holder = holder_of(entry);
    let title = |key: &str| tf(key, &[("0", Arg::from(lot.name.as_str())), ("1", Arg::from(holder.as_str()))]);
    let mut enter = false;
    let mut extra: Option<String> = None;
    let mut go_word = String::new();
    let mut with_buttons = true;
    match action {
        Action::BookOut | Action::Sell => {
            let selling = action == Action::Sell;
            ui.label(
                RichText::new(title(if selling { "s_sl_verkaufen_titel" } else { "s_sl_ausbuchen_titel" })).strong(),
            );
            field_label(ui, &t("s_sl_menge"));
            enter |= text_field(ui, scheme, &mut state.values.amount, "", &mut state.errors, "amount");
            small(ui, scheme, &tf("s_sl_frei", &[("0", Arg::from(amount_text(entry.free(), piece)))]), scheme.sub);
            if !entry.jobs.is_empty() || !entry.missions.is_empty() {
                small(ui, scheme, &t("s_sl_aus_reservierung_h"), scheme.sub);
                earmark_fields(ui, scheme, state, entry, piece);
            }
            if selling {
                field_label(ui, &t("s_sl_terminal"));
                enter |= text_field(
                    ui,
                    scheme,
                    &mut state.values.terminal,
                    &t("s_sl_terminal_h"),
                    &mut state.errors,
                    "terminal",
                );
                field_label(ui, &t("s_sl_erloes"));
                enter |=
                    text_field(ui, scheme, &mut state.values.proceeds, &t("s_sl_auec"), &mut state.errors, "proceeds");
            }
            go_word = t(if selling { "s_sl_verkaufen" } else { "s_sl_ausbuchen" });
        }
        Action::Move => {
            ui.label(RichText::new(title("s_sl_verschieben_titel")).strong());
            let writable: Vec<(String, String)> =
                data.writable_units().iter().map(|u| (u.id.clone(), u.label())).collect();
            field_label(ui, &t("s_sl_wie"));
            let mut modes = vec![(MoveMode::Location, t("s_sl_nach_ort")), (MoveMode::Holder, t("s_sl_an_mitglied"))];
            if writable.len() > 1 {
                modes.push((MoveMode::Pool, t("s_sl_in_einheit")));
            }
            if !modes.iter().any(|(m, _)| *m == state.values.mode) {
                state.values.mode = MoveMode::Location;
            }
            ui.horizontal_wrapped(|row| {
                for (mode, word) in &modes {
                    if widgets::radio(row, scheme, state.values.mode == *mode, word).clicked()
                        && state.values.mode != *mode
                    {
                        state.values.mode = *mode;
                        state.errors.clear();
                        if *mode == MoveMode::Pool {
                            state.values.to_unit.clear();
                        } else {
                            state.values.to_unit = lot.unit_id.clone();
                        }
                    }
                }
            });
            match state.values.mode {
                MoveMode::Pool => {
                    let others: Vec<(String, String)> =
                        writable.iter().filter(|(id, _)| *id != lot.unit_id).cloned().collect();
                    field_label(ui, &t("s_sl_ziel_einheit"));
                    let error = state.errors.get("to_unit").cloned();
                    if choice(
                        ui,
                        scheme,
                        "lager-ziel-einheit",
                        &t("s_sl_ziel_einheit"),
                        &others,
                        &mut state.values.to_unit,
                        error.as_ref(),
                    ) {
                        state.errors.remove("to_unit");
                    }
                    field_label(ui, &t("s_sl_menge"));
                    enter |=
                        text_field(ui, scheme, &mut state.values.amount, &t("s_sl_alles"), &mut state.errors, "amount");
                    small(ui, scheme, &t("s_sl_einheit_h"), scheme.sub);
                }
                MoveMode::Holder => {
                    field_label(ui, &t("s_sl_ziel_einheit"));
                    if choice(
                        ui,
                        scheme,
                        "lager-ziel-einheit-besitzer",
                        &t("s_sl_ziel_einheit"),
                        &writable,
                        &mut state.values.to_unit,
                        None,
                    ) {
                        state.values.holder.clear();
                    }
                    let roster = data.rosters.get(&state.values.to_unit).cloned().unwrap_or_default();
                    field_label(ui, &t("s_sl_besitzer"));
                    if roster.is_empty() {
                        small(ui, scheme, &t("s_sl_keine_mitglieder"), scheme.yellow);
                        field_error(ui, scheme, state.errors.get("holder"));
                    } else {
                        let entries: Vec<(String, String)> =
                            roster.iter().map(|m| (m.holder_ref.clone(), m.name.clone())).collect();
                        if !entries.iter().any(|(id, _)| *id == state.values.holder) {
                            state.values.holder = entries[0].0.clone();
                        }
                        let error = state.errors.get("holder").cloned();
                        choice(
                            ui,
                            scheme,
                            "lager-ziel-besitzer",
                            &t("s_sl_besitzer"),
                            &entries,
                            &mut state.values.holder,
                            error.as_ref(),
                        );
                    }
                }
                MoveMode::Location => {
                    field_label(ui, &t("s_sl_ort"));
                    let places = state.locations(Some(&ctx));
                    let (hit, _) = suggest_field(
                        ui,
                        scheme,
                        "lager-ziel-ort",
                        &t("s_sl_ort_h"),
                        &mut state.values.location,
                        &places,
                        &mut state.errors,
                        "location",
                    );
                    enter |= hit;
                    if state.locations_loading.is_some() {
                        small(ui, scheme, &t("s_sl_orte_laden"), scheme.sub);
                    }
                }
            }
            if state.values.mode != MoveMode::Pool {
                field_label(ui, &t("s_sl_menge"));
                enter |= text_field(ui, scheme, &mut state.values.amount, "", &mut state.errors, "amount");
                small(ui, scheme, &tf("s_sl_frei", &[("0", Arg::from(amount_text(entry.free(), piece)))]), scheme.sub);
                if !entry.jobs.is_empty() || !entry.missions.is_empty() {
                    small(ui, scheme, &t("s_sl_mit_reservierung_h"), scheme.sub);
                    earmark_fields(ui, scheme, state, entry, piece);
                }
            }
            if !piece {
                ui.add_space(4.0);
                widgets::switch_row(ui, scheme, &t("s_sl_zusammenlegen"), &mut state.values.merge);
            }
            go_word = t("s_sl_verschieben");
        }
        Action::Note => {
            ui.label(RichText::new(title("s_sl_notiz_titel")).strong());
            field_label(ui, &t("s_sl_notiz"));
            enter |= text_field(ui, scheme, &mut state.values.note, "", &mut state.errors, "note");
            form_message(ui, scheme, state);
            ui.add_space(GAP);
            with_buttons = false;
            ui.horizontal_wrapped(|row| {
                if row.add(widgets::Button::new(&t("s_sl_speichern"), ButtonKind::Strong, scheme)).clicked() || enter {
                    extra = Some("save".into());
                }
                if row.add(widgets::Button::new(&t("s_sl_notiz_weg"), ButtonKind::Normal, scheme)).clicked() {
                    extra = Some("clear".into());
                }
                if row.add(widgets::Button::new(&t("s_bt_abbrechen"), ButtonKind::Normal, scheme)).clicked() {
                    extra = Some("cancel".into());
                }
            });
        }
        Action::Stolen => {
            let key = if lot.stolen { "s_sl_nicht_gestohlen" } else { "s_sl_als_gestohlen" };
            ui.label(RichText::new(title(key)).strong());
            field_label(ui, &t("s_sl_menge"));
            enter |= text_field(ui, scheme, &mut state.values.amount, &t("s_sl_alles"), &mut state.errors, "amount");
            go_word = t("s_sl_markieren");
        }
        Action::Earmark => {
            ui.label(RichText::new(title("s_sl_reservieren_titel")).strong());
            let lines: Vec<(String, String)> = entry
                .jobs
                .iter()
                .map(|j| (format!("job:{}", j.id), job_label(j.number)))
                .chain(entry.missions.iter().map(|m| {
                    (format!("mission:{}", m.id), if m.name.is_empty() { "?".into() } else { m.name.clone() })
                }))
                .collect();
            for (id, label) in lines {
                field_label(ui, &label);
                ui.horizontal_wrapped(|row| {
                    let text = state.values.change.entry(id.clone()).or_default();
                    let field = widgets::text_field(text, &t("s_sl_menge"), scheme).id_salt(("reservierung", &id));
                    let response = row.add(field.desired_width(110.0));
                    let shown = text.clone();
                    response.widget_info(|| {
                        let mut info = WidgetInfo::text_edit(true, "", &shown, "");
                        info.label = Some(format!("{label}: {}", t("s_sl_menge")));
                        info
                    });
                    if row.add(widgets::Button::new(&t("s_sl_aendern"), ButtonKind::Normal, scheme)).clicked() {
                        extra = Some(format!("change:{id}"));
                    }
                    if row.add(widgets::Button::new(&t("s_sl_entfernen"), ButtonKind::Danger, scheme)).clicked() {
                        extra = Some(format!("remove:{id}"));
                    }
                });
            }
            field_error(ui, scheme, state.errors.get("change"));
            let found = if lot.material_bt.is_empty() {
                Some(Targets::default())
            } else {
                state.targets_for(Some(&ctx), &lot.unit_id, &lot.material_bt)
            };
            match found {
                None => small(ui, scheme, &t("s_sl_ziele_laden"), scheme.sub),
                Some(found) => {
                    let have: BTreeSet<&str> = entry
                        .jobs
                        .iter()
                        .map(|j| j.id.as_str())
                        .chain(entry.missions.iter().map(|m| m.id.as_str()))
                        .collect();
                    let mut choices: Vec<(String, String)> = found
                        .jobs
                        .iter()
                        .filter(|j| !have.contains(j.id.as_str()))
                        .map(|j| {
                            let status = match j.status.as_str() {
                                "OPEN" => t("s_bd_status_open"),
                                "IN_PROGRESS" => t("s_bd_status_in_progress"),
                                other => other.to_string(),
                            };
                            let number = j.number.map_or("?".to_string(), |n| n.to_string());
                            (format!("job:{}", j.id), format!("#{number} · {status}"))
                        })
                        .collect();
                    if !piece {
                        choices.extend(found.missions.iter().filter(|m| !have.contains(m.id.as_str())).map(|m| {
                            let name = if m.name.is_empty() { "?" } else { m.name.as_str() };
                            (format!("mission:{}", m.id), tf("s_sl_mission", &[("0", Arg::from(name))]))
                        }));
                    }
                    if choices.is_empty() {
                        small(ui, scheme, &t("s_sl_keine_ziele"), scheme.sub);
                    } else {
                        if !choices.iter().any(|(id, _)| *id == state.values.new_target) {
                            state.values.new_target = choices[0].0.clone();
                        }
                        field_label(ui, &t("s_sl_neu_fuer"));
                        let error = state.errors.get("new_target").cloned();
                        choice(
                            ui,
                            scheme,
                            "lager-neu-fuer",
                            &t("s_sl_neu_fuer"),
                            &choices,
                            &mut state.values.new_target,
                            error.as_ref(),
                        );
                        field_label(ui, &t("s_sl_menge"));
                        enter |=
                            text_field(ui, scheme, &mut state.values.new_amount, "", &mut state.errors, "new_amount");
                        if ui.add(widgets::Button::new(&t("s_sl_reservieren"), ButtonKind::Strong, scheme)).clicked()
                            || enter
                        {
                            extra = Some("add".into());
                        }
                    }
                }
            }
            form_message(ui, scheme, state);
            ui.add_space(GAP);
            with_buttons = false;
            if ui.add(widgets::Button::new(&t("s_bt_abbrechen"), ButtonKind::Normal, scheme)).clicked() {
                extra = Some("cancel".into());
            }
        }
    }
    if with_buttons {
        form_message(ui, scheme, state);
        let (go, cancel) = form_buttons(ui, scheme, &go_word);
        if go || enter {
            extra = Some("go".into());
        } else if cancel {
            extra = Some("cancel".into());
        }
    }
    match extra.as_deref() {
        None => {}
        Some("cancel") => {
            state.close_form();
            state.message = None;
        }
        Some("go") | Some("save") => state.book_entry(Some(&ctx), None),
        Some(other) => state.book_entry(Some(&ctx), Some(other)),
    }
}

/// How the table is laid out at this width.
struct Table {
    widths: [f32; 3],
    quality: bool,
    compact: bool,
}

fn text_width(ui: &Ui, text: &str, strong: bool) -> f32 {
    let rich = if strong { RichText::new(text).strong() } else { RichText::new(text) };
    egui::WidgetText::from(rich)
        .into_galley(
            ui,
            Some(egui::TextWrapMode::Extend),
            f32::INFINITY,
            egui::FontSelection::Style(egui::TextStyle::Body),
        )
        .size()
        .x
}

/// The cells of a lot besides its name: place, quality, amount.
fn lot_cells(lot: &Lot, quality: bool) -> [String; 3] {
    [
        if lot.place.is_empty() { "?".into() } else { lot.place.clone() },
        if quality { lot.quality.to_string() } else { String::new() },
        amount_text(lot.amount, lot.piece()),
    ]
}

fn list(ui: &mut Ui, scheme: &Scheme, state: &mut OrgStockState, data: &Arc<OrgStockData>) {
    let lots = state.visible_lots(data);
    if lots.is_empty() {
        widgets::empty_state(ui, scheme, "boxes", &t("s_sl_leer"));
        return;
    }
    let quality = !state.items;
    let heads = [t("s_ml_sp_ort"), t("s_ml_sp_q"), t("s_ml_sp_menge")];
    let mut widths = [0.0f32; 3];
    for (c, head) in heads.iter().enumerate() {
        widths[c] = text_width(ui, head, true) + 18.0;
    }
    for lot in &lots {
        for (c, text) in lot_cells(lot, quality).iter().enumerate() {
            widths[c] = widths[c].max(text_width(ui, text, false) + 16.0);
        }
    }
    if !quality {
        widths[1] = 0.0;
    }
    let available = ui.available_width() - crate::layout::SAFETY;
    let compact = available - widths.iter().sum::<f32>() - ACTIONS_WIDTH < NAME_MIN;
    let table = Table { widths, quality, compact };
    if !compact {
        list_heads(ui, scheme, &table, &heads, available);
    }
    let writable = data.can_write(&state.unit);
    let mut toggled = None;
    let mut open_form = None;
    widgets::list_frame_marked(ui, scheme, |frame| {
        frame.set_width(frame.available_width() - crate::layout::SAFETY);
        frame.spacing_mut().item_spacing.y = 0.0;
        for (n, lot) in lots.iter().enumerate() {
            let stripe = n % 2 == 1;
            let shown = state.open.contains(&lot.key);
            if lot_row(frame, scheme, state, lot, &table, stripe, shown) {
                toggled = Some((lot.key.clone(), shown));
            }
            if shown && let Some(entries) = &lot.entries {
                if entries.is_empty() {
                    frame.horizontal(|row| {
                        row.add_space(INDENT + 12.0);
                        small(row, scheme, &t("s_sl_keine_eintraege"), scheme.sub);
                    });
                }
                for entry in entries {
                    let lock = state.bookings_locked();
                    if let Some(action) = entry_row(frame, scheme, state, lot, entry, &table, stripe, writable, lock) {
                        open_form = Some(FormKey::Entry(action, entry.id.clone()));
                    }
                    if let Some(FormKey::Entry(action, id)) = state.form.clone()
                        && id == entry.id
                    {
                        frame.add_space(4.0);
                        frame.horizontal(|row| {
                            row.add_space(INDENT + 12.0);
                            row.vertical(|col| {
                                widgets::card(col, scheme, |card| {
                                    card.set_width(card.available_width());
                                    entry_form(card, scheme, state, data, action, lot, entry);
                                });
                            });
                        });
                        frame.add_space(6.0);
                    }
                }
            }
        }
    });
    if let Some((key, shown)) = toggled {
        if shown {
            state.open.remove(&key);
        } else {
            state.open.insert(key);
        }
    }
    if let Some(key) = open_form {
        state.open_form(key);
    }
    ui.add_space(GAP);
    small(ui, scheme, &t("s_sl_hinweis"), scheme.sub);
}

fn list_heads(ui: &mut Ui, scheme: &Scheme, table: &Table, heads: &[String; 3], available: f32) {
    let height = ui.text_style_height(&egui::TextStyle::Body) + 6.0;
    let (rect, _) = ui.allocate_exact_size(vec2(available, height), Sense::hover());
    let inner = rect.shrink2(vec2(11.0, 0.0));
    let mut x = inner.max.x - ACTIONS_WIDTH;
    for c in (0..3).rev() {
        if table.widths[c] == 0.0 {
            continue;
        }
        x -= table.widths[c];
        let spot = Rect::from_min_size(egui::pos2(x, rect.min.y), vec2(table.widths[c] - 8.0, height));
        let layout = if c == 0 { Layout::left_to_right(Align::Center) } else { Layout::right_to_left(Align::Center) };
        let mut child = ui.new_child(UiBuilder::new().max_rect(spot).layout(layout));
        child.label(RichText::new(heads[c].to_uppercase()).small().color(color(scheme.sub)));
    }
    let spot = Rect::from_min_max(inner.min, egui::pos2(x, inner.max.y));
    let mut child = ui.new_child(UiBuilder::new().max_rect(spot).layout(Layout::left_to_right(Align::Center)));
    child.label(RichText::new(t("s_ml_sp_bestand").to_uppercase()).small().color(color(scheme.sub)));
}

/// The badges of a lot: kind, stolen.
fn lot_badges(state: &OrgStockState, scheme: &Scheme, lot: &Lot) -> Vec<(String, Color)> {
    let mut marks = vec![(t(&state.kind_of(lot).text_key()), scheme.sub)];
    if lot.stolen {
        marks.push((t("s_sl_gestohlen"), scheme.gold));
    }
    marks
}

fn reserved_text(lot: &Lot) -> String {
    let mut parts = Vec::new();
    if lot.job > 0.0 {
        parts.push(tf("s_sl_res_auftraege", &[("0", Arg::from(amount_text(lot.job, lot.piece())))]));
    }
    if lot.mission > 0.0 {
        parts.push(tf("s_sl_res_missionen", &[("0", Arg::from(amount_text(lot.mission, lot.piece())))]));
    }
    parts.join(" · ")
}

/// The chip of a component: `Militär · S2 · C`.
fn component_chip(ui: &mut Ui, scheme: &Scheme, lot: &Lot) {
    let Some(c) = &lot.component else { return };
    let class = class_text(&c.class);
    let grade = c.grade.chars().next();
    match (c.size.and_then(|s| u8::try_from(s).ok()), grade) {
        (Some(size), Some(grade)) if c.grade.chars().count() == 1 => {
            widgets::class_badge(ui, scheme, class.as_deref(), size, grade);
        }
        _ => {
            let mut parts: Vec<String> = class.into_iter().collect();
            if let Some(size) = c.size {
                parts.push(format!("S{size}"));
            }
            if !c.grade.is_empty() {
                parts.push(c.grade.clone());
            }
            if !parts.is_empty() {
                widgets::badge(ui, scheme, scheme.sub, &parts.join(" · "));
            }
        }
    }
}

fn row_height(ui: &Ui, compact: bool) -> f32 {
    let text = ui.text_style_height(&egui::TextStyle::Body);
    let small = ui.text_style_height(&egui::TextStyle::Small);
    if compact { text + small + 16.0 } else { (text + 10.0).max(widgets::ICON_BUTTON_SIZE + 6.0) }
}

/// One lot; returns whether it was clicked to fold.
fn lot_row(
    ui: &mut Ui,
    scheme: &Scheme,
    state: &OrgStockState,
    lot: &Lot,
    table: &Table,
    stripe: bool,
    shown: bool,
) -> bool {
    let height = row_height(ui, table.compact);
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
    if !ui.is_rect_visible(rect) {
        return false;
    }
    let foldable = lot.entries.is_some();
    let id = ui.make_persistent_id(("lager-der-einheit-posten", &lot.key));
    let response = ui.interact(rect, id, if foldable { Sense::click() } else { Sense::hover() });
    let hovered = ui.rect_contains_pointer(rect);
    if hovered && foldable {
        ui.painter().rect_filled(rect, 0.0, color(scheme.hover));
    } else if stripe {
        ui.painter().rect_filled(rect, 0.0, color(scheme.bar));
    }
    let marks = lot_badges(state, scheme, lot);
    let cells = lot_cells(lot, table.quality);
    let reserved = reserved_text(lot);
    let mut spoken: Vec<String> = vec![lot.name.clone()];
    spoken.extend(marks.iter().map(|(m, _)| m.clone()));
    if let Some(c) = &lot.component {
        spoken.extend(class_text(&c.class));
        if let Some(size) = c.size {
            spoken.push(format!("S{size}"));
        }
        if !c.grade.is_empty() {
            spoken.push(c.grade.clone());
        }
    }
    spoken.extend(cells.iter().filter(|c| !c.is_empty()).cloned());
    if !reserved.is_empty() {
        spoken.push(reserved.clone());
    }
    if foldable {
        spoken.push(t(if shown { "s_mlg_offen" } else { "s_mlg_zu" }));
    }
    let spoken = spoken.join(", ");
    response.widget_info(|| WidgetInfo::labeled(Role::Row, true, &spoken));

    let inner = rect.shrink2(vec2(6.0, 2.0));
    let actions = Rect::from_min_max(egui::pos2(inner.max.x - ACTIONS_WIDTH, inner.min.y), inner.max);
    let name_right = if table.compact { actions.min.x } else { actions.min.x - table.widths.iter().sum::<f32>() };
    let line = if table.compact { (inner.height() - 2.0) / 2.0 } else { inner.height() };
    let name_rect = Rect::from_min_max(inner.min, egui::pos2(name_right, inner.min.y + line));
    let mut cell = ui.new_child(UiBuilder::new().max_rect(name_rect).layout(Layout::left_to_right(Align::Center)));
    let tone = if lot.stolen { scheme.gold } else { scheme.fg };
    let room = (cell.available_width() * 0.55).max(60.0);
    cell.scope(|c| {
        c.set_max_width(room);
        c.add(egui::Label::new(RichText::new(&lot.name).strong().color(color(tone))).truncate());
    });
    component_chip(&mut cell, scheme, lot);
    let refs: Vec<(&str, Color)> = marks.iter().map(|(m, c)| (m.as_str(), *c)).collect();
    widgets::badges(&mut cell, scheme, &refs);
    if !table.compact && !reserved.is_empty() {
        cell.add(egui::Label::new(RichText::new(&reserved).small().color(color(scheme.sub))).truncate());
    }
    if table.compact {
        let spot = Rect::from_min_max(egui::pos2(inner.min.x, name_rect.max.y), egui::pos2(actions.min.x, inner.max.y));
        let mut sub = ui.new_child(UiBuilder::new().max_rect(spot).layout(Layout::left_to_right(Align::Center)));
        let mut parts: Vec<String> = cells.iter().filter(|c| !c.is_empty()).cloned().collect();
        if !reserved.is_empty() {
            parts.push(reserved);
        }
        sub.add(egui::Label::new(RichText::new(parts.join("  ·  ")).small().color(color(scheme.sub))).truncate());
    } else {
        let mut x = actions.min.x;
        for c in (0..3).rev() {
            if table.widths[c] == 0.0 {
                continue;
            }
            x -= table.widths[c];
            let spot = Rect::from_min_size(egui::pos2(x, inner.min.y), vec2(table.widths[c] - 8.0, inner.height()));
            let layout =
                if c == 0 { Layout::left_to_right(Align::Center) } else { Layout::right_to_left(Align::Center) };
            let mut child = ui.new_child(UiBuilder::new().max_rect(spot).layout(layout));
            let tone = if c == 2 { scheme.fg } else { scheme.sub };
            child.add(egui::Label::new(RichText::new(&cells[c]).color(color(tone))).truncate());
        }
    }
    if foldable {
        let mut buttons = ui.new_child(UiBuilder::new().max_rect(actions).layout(Layout::right_to_left(Align::Center)));
        let icon = if shown { "chevron-down" } else { "chevron-right" };
        let tint = if hovered { IconTint::Hover } else { IconTint::Normal };
        if let Some(image) = crate::icons::themed(icon, scheme, tint, 16.0) {
            buttons.add(image);
        }
        return response.on_hover_cursor(egui::CursorIcon::PointingHand).clicked();
    }
    false
}

/// One entry of an open lot; returns the booking whose icon was clicked.
#[allow(clippy::too_many_arguments)]
fn entry_row(
    ui: &mut Ui,
    scheme: &Scheme,
    state: &OrgStockState,
    lot: &Lot,
    entry: &Entry,
    table: &Table,
    stripe: bool,
    writable: bool,
    lock: Option<&str>,
) -> Option<Action> {
    let mut details = Vec::new();
    if !entry.note.is_empty() {
        details.push(tf("s_sl_notiz_text", &[("0", Arg::from(entry.note.as_str()))]));
    }
    for job in &entry.jobs {
        let mut text = tf(
            "s_sl_fuer_auftrag",
            &[("0", Arg::from(amount_text(job.amount, lot.piece()))), ("1", Arg::from(job_label(job.number)))],
        );
        if job.delivered {
            text = format!("{text} {}", t("s_sl_geliefert"));
        }
        details.push(text);
    }
    for mission in &entry.missions {
        let name = if mission.name.is_empty() { "?" } else { mission.name.as_str() };
        details.push(tf(
            "s_sl_fuer_mission",
            &[("0", Arg::from(amount_text(mission.amount, lot.piece()))), ("1", Arg::from(name))],
        ));
    }
    let details = details.join(" · ");
    let height = if table.compact || !details.is_empty() { row_height(ui, true) } else { row_height(ui, false) };
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
    if !ui.is_rect_visible(rect) {
        return None;
    }
    let id = ui.make_persistent_id(("lager-der-einheit-eintrag", &entry.id));
    // Reachable by keyboard: with the focus on the row or one of its icons
    // the icons show as under the mouse.
    let response = ui.interact(rect, id, Sense::focusable_noninteractive());
    let icons_focused = ui.ctx().data(|d| d.get_temp::<bool>(id.with("symbole")).unwrap_or(false));
    let hovered = ui.rect_contains_pointer(rect) || response.has_focus() || icons_focused;
    let active = match &state.form {
        Some(FormKey::Entry(action, id)) if *id == entry.id => Some(*action),
        _ => None,
    };
    if active.is_some() {
        ui.painter().rect_filled(rect, 0.0, color(scheme.accent_dark));
        let bar = Rect::from_min_size(rect.min, vec2(widgets::LIST_BAR_WIDTH, rect.height()));
        ui.painter().rect_filled(bar, 0.0, color(scheme.accent));
    } else if stripe {
        ui.painter().rect_filled(rect, 0.0, color(scheme.bar));
    }
    let holder = holder_of(entry);
    let amount = amount_text(entry.amount, lot.piece());
    let spoken = [holder.as_str(), details.as_str(), amount.as_str()]
        .iter()
        .filter(|s| !s.is_empty())
        .copied()
        .collect::<Vec<_>>()
        .join(", ");
    response.widget_info(|| WidgetInfo::labeled(Role::Row, true, &spoken));
    let inner = rect.shrink2(vec2(6.0, 2.0));
    let actions = Rect::from_min_max(egui::pos2(inner.max.x - ACTIONS_WIDTH, inner.min.y), inner.max);
    let amount_left = actions.min.x - if table.compact { 0.0 } else { table.widths[2] };
    let half = (inner.height() - 2.0) / 2.0;
    let two_lines = !details.is_empty() || table.compact;
    let name_rect = Rect::from_min_max(
        egui::pos2(inner.min.x + INDENT, inner.min.y),
        egui::pos2(amount_left, if two_lines { inner.min.y + half } else { inner.max.y }),
    );
    let mut cell = ui.new_child(UiBuilder::new().max_rect(name_rect).layout(Layout::left_to_right(Align::Center)));
    cell.add(egui::Label::new(RichText::new(&holder)).truncate());
    if two_lines {
        let spot =
            Rect::from_min_max(egui::pos2(inner.min.x + INDENT, name_rect.max.y), egui::pos2(amount_left, inner.max.y));
        let mut sub = ui.new_child(UiBuilder::new().max_rect(spot).layout(Layout::left_to_right(Align::Center)));
        let line = if table.compact && details.is_empty() {
            amount.clone()
        } else if table.compact {
            format!("{amount}  ·  {details}")
        } else {
            details.clone()
        };
        sub.add(egui::Label::new(RichText::new(line).small().color(color(scheme.sub))).truncate());
    }
    if !table.compact {
        let spot =
            Rect::from_min_size(egui::pos2(amount_left, inner.min.y), vec2(table.widths[2] - 8.0, inner.height()));
        let mut child = ui.new_child(UiBuilder::new().max_rect(spot).layout(Layout::right_to_left(Align::Center)));
        child.add(egui::Label::new(RichText::new(&amount)).truncate());
    }
    if !writable || !(hovered || active.is_some()) {
        ui.ctx().data_mut(|d| d.remove::<bool>(id.with("symbole")));
        return None;
    }
    let mut buttons = ui.new_child(UiBuilder::new().max_rect(actions).layout(Layout::right_to_left(Align::Center)));
    buttons.spacing_mut().item_spacing.x = 4.0;
    let mut clicked = None;
    let mut focused = false;
    for (action, icon, meaning) in ENTRY_ACTIONS.iter().rev() {
        // Locked: the hover says why nothing happens, and a click opens no form.
        let meaning = lock.map_or_else(|| t(meaning), str::to_string);
        let icon = named_icon(&mut buttons, scheme, icon, &meaning, active == Some(*action));
        focused |= icon.has_focus();
        if icon.clicked() {
            clicked = Some(*action);
        }
    }
    ui.ctx().data_mut(|d| d.insert_temp(id.with("symbole"), focused));
    clicked
}

#[cfg(test)]
mod tests {
    use super::*;
    use versekit_core::language::Lang;

    /// The page against the made-up Basetool, data fetched.
    fn ready() -> OrgStockState {
        ready_with(Link::demo())
    }

    fn ready_with(link: Link) -> OrgStockState {
        let mut state = OrgStockState::new(Some(link), None);
        state.fetch_now(None);
        let answer = state.fetching.as_ref().and_then(Pending::wait).expect("fetched");
        state.fetching = None;
        state.handle(answer);
        state.tick(None);
        assert!(state.data.is_some(), "no data: {:?}", state.fetch_error);
        state
    }

    /// Whether `text` is the text of `key` in either language: other tests
    /// switch the language of the whole process while these run.
    fn says(text: Option<&String>, key: &str, args: &[(&str, &str)]) -> bool {
        let args: Vec<(&str, Arg)> = args.iter().map(|(k, v)| (*k, Arg::from(*v))).collect();
        text.is_some_and(|text| [Lang::De, Lang::En].iter().any(|lang| language::tf_in(*lang, key, &args) == *text))
    }

    fn said(state: &OrgStockState, key: &str, args: &[(&str, &str)], tone: Tone) -> bool {
        state.message.as_ref().is_some_and(|(text, t)| *t == tone && says(Some(text), key, args))
    }

    /// Waits for the running request and takes its answer.
    fn settle(state: &mut OrgStockState) {
        if let Some(answer) = state.busy.as_ref().and_then(Pending::wait) {
            state.busy = None;
            state.handle(answer);
        }
        state.tick(None);
    }

    fn draw(state: &mut OrgStockState, width: f32) -> f32 {
        let scheme = versekit_core::theme::current();
        let ctx = egui::Context::default();
        crate::install(&ctx);
        let mut overflow = 0.0;
        for _ in 0..3 {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, 1600.0))),
                ..Default::default()
            };
            let mut output = ctx.run_ui(input, |ui| {
                overflow = crate::layout::page(ui, scheme, "seite", |page| show_page(page, scheme, state)).overflow;
            });
            output.textures_delta.clear();
        }
        overflow
    }

    #[test]
    fn the_first_unit_tabs_kinds_and_search() {
        let mut state = ready();
        assert_eq!(state.unit, "u-alpha", "first unit when none was chosen");
        let data = state.data.clone().unwrap();
        let names: Vec<&str> = state.visible_lots(&data).iter().map(|l| l.name.as_str()).collect();
        assert_eq!(names, ["Agricium (Ore)", "Hadanite", "Laranite", "Quantainium"]);
        state.items = true;
        let names: Vec<&str> = state.visible_lots(&data).iter().map(|l| l.name.as_str()).collect();
        assert_eq!(names, ["Bulwark", "FR-66", "Pembroke Helm"]);
        let fr66 = data.lots.iter().find(|l| l.name == "FR-66").unwrap();
        assert_eq!(state.kind_of(fr66), Kind::VehicleItem, "a shield is a vehicle item");
        state.items = false;
        state.search = "area".into();
        assert!(state.is_narrowed());
        let names: Vec<&str> = state.visible_lots(&data).iter().map(|l| l.name.as_str()).collect();
        assert_eq!(names, ["Hadanite", "Laranite"]);
        state.reset();
        assert!(!state.is_narrowed());
        assert_eq!(draw(&mut state, 1100.0), 0.0);
        assert_eq!(draw(&mut state, 420.0), 0.0, "narrow: two lines, nothing sticks out");
    }

    #[test]
    fn a_sync_keeps_form_typing_filters_and_open_lots() {
        let mut state = ready();
        state.search = "lara".into();
        state.place = "Area18".into();
        state.open.insert("L-1".into());
        state.open_form(FormKey::Entry(Action::Note, "e-2".into()));
        state.values.note = "halb getippt".into();
        let before = state.seen;
        state.fetch_now(None);
        let answer = state.fetching.as_ref().and_then(Pending::wait).expect("fetched");
        state.fetching = None;
        state.handle(answer);
        state.tick(None);
        assert!(state.seen > before, "the sync brought new data");
        assert_eq!(state.data().and_then(|d| d.entry("e-1")).map(|(_, e)| e.version), Some(2), "change from outside");
        assert_eq!(state.form, Some(FormKey::Entry(Action::Note, "e-2".into())));
        assert_eq!(state.values.note, "halb getippt");
        assert_eq!((state.search.as_str(), state.place.as_str()), ("lara", "Area18"));
        assert!(state.open.contains("L-1"));
    }

    #[test]
    fn booking_out_closes_the_form_and_fetches() {
        let mut state = ready();
        state.open_form(FormKey::Entry(Action::BookOut, "e-4".into()));
        assert_eq!(state.values.amount, "32", "the free part comes in");
        state.values.amount = "33".into();
        state.book_entry(None, None);
        assert!(state.errors.contains_key("amount"), "more than the entry holds is refused here");
        assert!(state.busy.is_none(), "nothing went out");
        state.values.amount = "2,5".into();
        state.book_entry(None, None);
        settle(&mut state);
        assert_eq!(state.form, None);
        assert_eq!(state.data().and_then(|d| d.entry("e-4")).map(|(_, e)| e.amount), Some(29.5));
    }

    #[test]
    fn moving_to_another_unit_without_a_target_says_so() {
        let mut state = ready();
        state.open_form(FormKey::Entry(Action::Move, "e-4".into()));
        state.values.mode = MoveMode::Pool;
        state.values.to_unit.clear();
        state.book_entry(None, None);
        assert!(says(state.errors.get("to_unit"), "s_sle_ziel_fehlt", &[]), "{:?}", state.errors);
        assert!(state.busy.is_none());
        state.values.to_unit = "u-fracht".into();
        state.values.amount.clear();
        state.book_entry(None, None);
        settle(&mut state);
        let data = state.data().unwrap();
        assert_eq!(data.entry("e-4").map(|(lot, _)| lot.unit_id.as_str()), Some("u-fracht"), "the whole entry moved");
    }

    #[test]
    fn a_stale_version_is_refused_and_the_new_state_comes() {
        let (link, demo) = Link::demo_with();
        let mut state = ready_with(link);
        state.open_form(FormKey::Entry(Action::Note, "e-1".into()));
        state.values.note = "meine".into();
        // Another member changes e-1 meanwhile; the page takes the new state
        // in, the form stays and still names the version it opened on.
        state.fetch_now(None);
        let answer = state.fetching.as_ref().and_then(Pending::wait).expect("fetched");
        state.fetching = None;
        state.handle(answer);
        state.tick(None);
        assert_eq!(state.data().and_then(|d| d.entry("e-1")).map(|(_, e)| e.version), Some(2));
        state.book_entry(None, None);
        settle(&mut state);
        let sent = demo.change_sets();
        let version = sent.last().and_then(|s| s.get("ops")).and_then(|o| match o {
            Value::Array(ops) => ops.first().and_then(|op| op.get("version")).cloned(),
            _ => None,
        });
        assert_eq!(version, Some(Value::int(1)), "the version the form opened on");
        assert!(said(&state, "s_sle_r_version_conflict", &[], Tone::Error), "{:?}", state.message);
        assert_eq!(state.form, Some(FormKey::Entry(Action::Note, "e-1".into())), "a refusal keeps the form");
        assert_eq!(state.values.note, "meine");
        // The new state came with the refusal: the second try goes through.
        state.book_entry(None, None);
        settle(&mut state);
        assert_eq!(state.form, None, "booked with the new version");
        assert_eq!(state.data().and_then(|d| d.entry("e-1")).map(|(_, e)| e.note.clone()), Some("meine".into()));
    }

    #[test]
    fn a_locked_storage_books_nothing_and_says_why() {
        let reason = "In der Testumgebung wird beim echten Basetool nichts gebucht. Mit Live-Daten geht es.";
        let mut state = ready();
        state.lock_bookings(Some(reason.to_string()));
        assert_eq!(state.bookings_locked(), Some(reason));
        // No form opens, and the reason is the answer to the click.
        for key in [
            FormKey::BookIn("u-alpha".into()),
            FormKey::Entry(Action::BookOut, "e-1".into()),
            FormKey::Entry(Action::Sell, "e-1".into()),
            FormKey::Entry(Action::Move, "e-1".into()),
            FormKey::Entry(Action::Note, "e-1".into()),
            FormKey::Entry(Action::Stolen, "e-1".into()),
            FormKey::Entry(Action::Earmark, "e-1".into()),
        ] {
            state.open_form(key.clone());
            assert_eq!(state.form, None, "{key:?} must not open");
            assert!(state.message.as_ref().is_some_and(|(text, tone)| text == reason && *tone == Tone::Warn));
        }
        // Even a form that was open before the lock sends nothing.
        let mut state = ready();
        state.open_form(FormKey::Entry(Action::Note, "e-1".into()));
        state.values.note = "gesperrt".into();
        state.book_lock = Some(reason.to_string());
        state.book_entry(None, None);
        assert!(!state.busy(), "nothing was sent");
        assert_ne!(state.data().and_then(|d| d.entry("e-1")).map(|(_, e)| e.note.clone()), Some("gesperrt".into()));
        // Counter-check: without the lock the same form opens and books.
        let mut state = ready();
        state.lock_bookings(Some(reason.to_string()));
        state.lock_bookings(None);
        state.open_form(FormKey::Entry(Action::Note, "e-1".into()));
        assert_eq!(state.form, Some(FormKey::Entry(Action::Note, "e-1".into())), "the lock lifts again");
        state.values.note = "frei".into();
        state.book_entry(None, None);
        settle(&mut state);
        assert_eq!(state.data().and_then(|d| d.entry("e-1")).map(|(_, e)| e.note.clone()), Some("frei".into()));
    }

    #[test]
    fn a_booking_closes_only_its_own_form() {
        let mut state = ready();
        state.open_form(FormKey::Entry(Action::BookOut, "e-4".into()));
        state.values.amount = "1".into();
        state.book_entry(None, None);
        // While it is on its way, another form opens and gets typed into.
        state.open_form(FormKey::Entry(Action::Note, "e-6".into()));
        state.values.note = "halb".into();
        settle(&mut state);
        assert_eq!(state.form, Some(FormKey::Entry(Action::Note, "e-6".into())));
        assert_eq!(state.values.note, "halb");
        assert_eq!(state.data().and_then(|d| d.entry("e-4")).map(|(_, e)| e.amount), Some(31.0));
    }

    #[test]
    fn cancelling_during_the_lookup_books_nothing() {
        let (link, demo) = Link::demo_with();
        let mut state = ready_with(link);
        state.open_form(FormKey::BookIn("u-alpha".into()));
        // Not in any pool and not in the goods list: it is looked up.
        state.values.material = "Bulwar".into();
        state.values.location = "Area18".into();
        state.values.amount = "1".into();
        state.values.piece = true;
        state.book_in(None);
        assert!(state.busy() && state.ticket != 0, "the lookup runs");
        // Cancel: the form closes before the answer comes.
        state.open_form(FormKey::BookIn("u-alpha".into()));
        assert!(!state.busy(), "the lookup no longer holds the page");
        std::thread::sleep(Duration::from_millis(300));
        state.tick(None);
        assert!(demo.change_sets().is_empty(), "booked after cancelling: {:?}", demo.change_sets());
    }

    #[test]
    fn booking_in_looks_up_and_offers_hits() {
        let mut state = ready();
        state.open_form(FormKey::BookIn("u-alpha".into()));
        assert_eq!(state.values.holder, "r-player", "the own entry");
        state.book_in(None);
        assert!(["material", "location", "amount"].iter().all(|f| state.errors.contains_key(f)));
        state.values.material = "Agri".into();
        state.values.location = "Area18".into();
        state.values.amount = "4".into();
        state.values.quality = "1001".into();
        state.book_in(None);
        assert!(state.errors.contains_key("quality"));
        state.values.quality = "700".into();
        state.book_in(None);
        settle(&mut state);
        assert_eq!(state.values.candidates.len(), 2, "Agricium and Agricium (Ore)");
        let hit = state.values.candidates.iter().find(|c| c.name == "Agricium").cloned().unwrap();
        state.choose_candidate(hit);
        state.book_in(None);
        settle(&mut state);
        assert_eq!(state.form, None, "booked");
        let data = state.data().unwrap();
        assert!(data.lots.iter().any(|l| l.name == "Agricium" && l.unit_id == "u-alpha" && l.quality == 700));
        // An item in SCU is refused before anything goes out.
        state.open_form(FormKey::BookIn("u-alpha".into()));
        state.values = Values {
            holder: "r-nova".into(),
            material: "FR-66".into(),
            location: "Lorville".into(),
            amount: "1".into(),
            ..Values::default()
        };
        state.book_in(None);
        settle(&mut state);
        assert!(said(&state, "s_sle_ware_stueck", &[("0", "FR-66")], Tone::Error), "{:?}", state.message);
    }

    #[test]
    fn a_changed_form_during_the_lookup_books_nothing() {
        let mut state = ready();
        state.open_form(FormKey::BookIn("u-alpha".into()));
        state.values.material = "Quantainium-neu".into();
        state.values.location = "Area18".into();
        state.values.amount = "1".into();
        state.book_in(None);
        state.values.amount = "2".into();
        settle(&mut state);
        assert!(said(&state, "s_sle_ware_geaendert", &[], Tone::Warn), "{:?}", state.message);
    }

    #[test]
    fn a_large_booking_shows_the_confirmation() {
        let mut state = ready();
        state.open_form(FormKey::Entry(Action::Note, "e-10".into()));
        let data = state.data().unwrap().clone();
        let (lot, entry) = data.entry("e-10").unwrap();
        let op = orgstock::op_book_out(&lot.unit_id, entry, 9000.0, SCU, &Earmarks::new(), &Earmarks::new());
        state.submit(None, vec![op]);
        settle(&mut state);
        assert!(state.confirm.as_ref().is_some_and(|(url, _)| url.ends_with("/bestaetigen/demo")));
        assert!(said(&state, "s_sle_masse", &[], Tone::Warn), "{:?}", state.message);
        state.confirm.as_mut().unwrap().1 = 0.0;
        draw(&mut state, 1100.0);
        assert!(state.confirm.is_none(), "gone once expired");
    }

    #[test]
    fn every_reason_has_a_text_without_long_dash() {
        for code in orgstock::REASONS {
            let key = reason_key(code).unwrap_or_default();
            for lang in [Lang::De, Lang::En] {
                let text = language::text_in(lang, key).unwrap_or_default();
                assert!(!text.is_empty() && !text.contains('—'), "{code}: {text}");
            }
        }
        assert!(says(Some(&reason_text("NEU")), "s_sl_r_anders", &[("0", "NEU")]));
    }

    #[test]
    fn forms_draw_without_sticking_out() {
        let mut state = ready();
        state.open.insert("L-1".into());
        for key in [
            FormKey::BookIn("u-alpha".into()),
            FormKey::Entry(Action::BookOut, "e-1".into()),
            FormKey::Entry(Action::Sell, "e-1".into()),
            FormKey::Entry(Action::Move, "e-1".into()),
            FormKey::Entry(Action::Note, "e-1".into()),
            FormKey::Entry(Action::Stolen, "e-1".into()),
            FormKey::Entry(Action::Earmark, "e-1".into()),
        ] {
            state.form = None;
            state.open_form(key.clone());
            for width in [1100.0, 430.0] {
                assert_eq!(draw(&mut state, width), 0.0, "{key:?} at {width}");
            }
        }
    }

    /// On the sync of the program the page fetches through it, and the sync
    /// button says how the pass went once it is over: first that it runs, then
    /// the result, which names a held change set before the time.
    #[test]
    fn the_sync_button_reports_the_pass() {
        use crate::basetool_page::Wire;
        use versekit_core::basetool::demo::DemoLogin;
        use versekit_core::basetool::overview::sync_result;
        use versekit_core::basetool::scopes::SETTING_ORGSTOCK;
        let dir = std::env::temp_dir().join(format!("versekit-lager-sync-{}", versekit_core::testdir::run_tag()));
        let _ = std::fs::remove_dir_all(&dir);
        let wire = Wire::in_folder(&dir.to_string_lossy(), false);
        wire.demo.as_ref().expect("demo").script_login(DemoLogin::Approve, 1);
        let sync = wire.sync.clone();
        assert!(sync.set_setting(SETTING_ORGSTOCK, Value::Bool(true)));
        sync.connect();
        let end = std::time::Instant::now() + Duration::from_secs(20);
        while !sync.connection().connected() && std::time::Instant::now() < end {
            std::thread::sleep(Duration::from_millis(50));
        }
        assert!(sync.connection().connected());
        let mut state = OrgStockState::new(Some(Link::from_wire(&wire)), None);
        state.sync_now(None);
        assert!(state.notice.as_ref().is_some_and(|(text, _)| says(Some(text), "s_btv_sync_laeuft", &[])));
        sync.pass_now().expect("pass");
        state.tick(None);
        let status = sync.status().get().1.expect("status");
        let expected = sync_result(&status, &sync.state().unwrap_or_default(), Some(SETTING_ORGSTOCK), sync.now()).0;
        assert_eq!(state.notice.as_ref().map(|(text, _)| text.clone()), Some(expected));
        assert_eq!(state.notice_icon, "check");
        assert!(state.data.is_some(), "the data came through the sync");
        // Another pass the button did not ask for says nothing new.
        state.notice = None;
        sync.pass_now().expect("pass");
        state.tick(None);
        assert!(state.notice.is_none());
        // The result of the pass from before the click is not the answer to
        // it: the notice says that it runs until a later pass is over.
        state.sync_now(None);
        state.tick(None);
        assert!(
            state.notice.as_ref().is_some_and(|(text, _)| says(Some(text), "s_btv_sync_laeuft", &[])),
            "an old result stands in for the answer"
        );
        sync.pass_now().expect("pass");
        state.tick(None);
        assert_eq!(state.notice_icon, "check");
        assert!(!state.notice.as_ref().is_some_and(|(text, _)| says(Some(text), "s_btv_sync_laeuft", &[])));
        let _ = std::fs::remove_dir_all(dir);
    }
}
