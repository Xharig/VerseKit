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

//! Personal and shared on the stock page (`my_stock_page.py`,
//! `stock_split.py`): with a connection each row knows how much of its lot
//! lies in the pool of a unit; the page can show all, the personal or the
//! shared part, and share into a pool or take back.
//!
//! ⚠ Without a connection (area off, right missing, not connected, stock
//! never synced) there is no split: everything counts as personal, the view
//! stays on "All" and the page shows no switch and no buttons.
//!
//! The form stands above the list, not in it: the list holds fixed row
//! heights and draws only what is in view.

use std::collections::HashMap;
use std::sync::mpsc::{self, Receiver};

use egui::{RichText, Ui};
use versekit_core::basetool::error::error_text;
use versekit_core::basetool::orgunits::{OrgUnit, label as unit_label};
use versekit_core::basetool::scopes::SETTING_STOCK;
use versekit_core::basetool::stock_split::{
    self, Failure, Kind, PIECE, RowRef, Scope, Share, Snapshot, check_amount, reason_key, share_op, split_of,
    unshare_op,
};
use versekit_core::basetool::sync::{Shared, SyncHandle};
use versekit_core::jsonio::Value;
use versekit_core::language::{Arg, t, tf};
use versekit_core::stock::{Row, Store};
use versekit_core::theme::{Color, Scheme};

use crate::color;
use crate::widgets::{self, ButtonKind};

/// What reaches the page: the sync of the program and the units it fetched.
#[derive(Clone)]
pub struct Link {
    pub sync: SyncHandle,
    pub units: Shared<Vec<OrgUnit>>,
}

impl Link {
    pub fn from_wire(wire: &crate::basetool_page::Wire) -> Link {
        Link { sync: wire.sync.clone(), units: wire.units.clone() }
    }
}

/// The open form: which row, share or take back, the unit and the typed amount.
struct Form {
    store: Store,
    index: usize,
    kind: Kind,
    name: String,
    /// The lot the form was opened for: a row that moved to another lot closes it.
    lot: String,
    unit: String,
    amount: String,
    /// The unit the amount in the field belongs to.
    amount_for: String,
    error: Option<String>,
}

/// An op on its way and what it was.
struct Sending {
    kind: Kind,
    answer: Receiver<Result<(), Failure>>,
    /// Sent because a booking asked for it: a refusal leaves the booking personal.
    booked: bool,
}

/// A booking that waits for its lot at the Basetool to give a part of it into a unit.
struct Wish {
    store: Store,
    name: String,
    place: String,
    quality: Option<i64>,
    stolen: bool,
    unit: String,
    /// How much goes into the unit.
    amount: f64,
    /// When the booking was made (time of the sync).
    since: f64,
}

/// How long a booking waits for its lot at the Basetool (seconds).
const WISH_WAIT_SECONDS: f64 = 180.0;

/// The amount typed for the unit is wrong; the field says so.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WrongAmount;

/// What the field "For" of a booking asks for.
#[derive(Clone, Debug, PartialEq)]
pub struct For {
    pub store: Store,
    pub name: String,
    pub place: String,
    pub quality: Option<i64>,
    pub stolen: bool,
    pub unit: String,
    pub amount: f64,
}

/// The split of the stock page.
#[derive(Default)]
pub struct Split {
    link: Option<Link>,
    /// The generation of the sync status the snapshot was taken at.
    seen: Option<u64>,
    snapshot: Option<Snapshot>,
    units: Vec<OrgUnit>,
    units_fetched: bool,
    shares: HashMap<(Store, usize), Share>,
    pub scope: Scope,
    form: Option<Form>,
    sending: Option<Sending>,
    /// The field "For" of the booking forms: a unit (empty: only me) and the amount for it (empty: all).
    for_unit: String,
    for_amount: String,
    for_error: Option<String>,
    /// The bookings waiting for their lot, oldest first.
    wishes: Vec<Wish>,
    /// What the page is to say next, from a booking that ended without a send.
    said: Option<String>,
}

fn amount_text(value: f64, piece: bool) -> String {
    versekit_core::formats::amount(
        versekit_core::language::current(),
        value,
        &if piece { t("s_lg_stueck") } else { "SCU".to_string() },
    )
}

fn field_text(value: f64) -> String {
    let text = format!("{:.4}", versekit_core::stock::py_round(value, 4));
    let text = text.trim_end_matches('0').trim_end_matches('.');
    text.replace('.', &versekit_core::formats::decimal_mark(versekit_core::language::current()).to_string())
}

impl Split {
    pub fn new(link: Option<Link>) -> Split {
        Split { link, ..Split::default() }
    }

    /// Whether there is a split to show.
    pub fn available(&self) -> bool {
        self.snapshot.is_some()
    }

    /// Takes the state of the last sync and gives every row its part.
    /// `raw` and `trade` are the entries the rows stand for.
    pub fn refresh(&mut self, ctx: &egui::Context, rows: &[Row], raw: &[Value], trade: &[Value]) {
        let Some(link) = self.link.clone() else { return };
        let linked = link.sync.area_visible(SETTING_STOCK);
        let generation = link.sync.status().generation();
        if !linked {
            self.snapshot = None;
            self.seen = None;
        } else if self.seen != Some(generation) || self.snapshot.is_none() {
            self.seen = Some(generation);
            self.snapshot = link.sync.state().and_then(|state| stock_split::snapshot(&state));
        }
        let (_, list) = link.units.get();
        self.units_fetched = list.is_some();
        self.units = list.map(|l| (*l).clone()).unwrap_or_default();
        let Some(snapshot) = &self.snapshot else {
            self.scope = Scope::All;
            self.shares.clear();
            self.form = None;
            return;
        };
        let refs: Vec<RowRef> = rows
            .iter()
            .map(|row| {
                let entry = match row.store {
                    Store::Raw => raw.get(row.index),
                    Store::Trade => trade.get(row.index),
                };
                let lookup = match (row.store, entry) {
                    (Store::Trade, Some(entry)) => versekit_core::basetool::stock_sync::trade_lookup(entry),
                    _ => row.name.clone(),
                };
                RowRef {
                    lookup,
                    raw: row.store == Store::Raw,
                    item: row.item,
                    quality: row.quality,
                    place: row.place.clone(),
                    stolen: row.stolen,
                    amount: row.amount,
                }
            })
            .collect();
        let parts = stock_split::attach(&refs, Some(snapshot));
        self.shares = rows.iter().zip(parts).map(|(row, part)| ((row.store, row.index), part)).collect();
        // A form belongs to the lot it was opened for.
        let stale = self.form.as_ref().is_some_and(|form| {
            let here = rows.iter().find(|r| (r.store, r.index) == (form.store, form.index));
            let lot = here.and_then(|r| self.shares.get(&(r.store, r.index))).and_then(|s| s.lot.as_ref());
            here.is_none_or(|r| r.name != form.name) || lot != Some(&form.lot)
        });
        if stale {
            self.form = None;
        }
        self.fire_wish(ctx, &link, rows);
    }

    /// Whether the booking forms offer "For": a connection, the stock synced, and a unit to give into.
    pub fn offered(&self) -> bool {
        self.available() && !self.units.is_empty()
    }

    /// Takes what the field "For" asks of a booking of `booked` (in the unit of the lot): nothing when
    /// it is only for me; `Err` when the amount for the unit is wrong (said at the field, nothing is booked).
    pub fn choose(&mut self, booked: f64, piece: bool) -> Result<Option<(String, f64)>, WrongAmount> {
        self.for_error = None;
        if !self.offered() || self.for_unit.is_empty() || !self.units.iter().any(|u| u.id == self.for_unit) {
            return Ok(None);
        }
        let typed = self.for_amount.trim();
        let amount = if typed.is_empty() { Some(booked) } else { check_amount(typed, booked, piece) };
        match amount {
            Some(amount) => Ok(Some((self.for_unit.clone(), amount))),
            None => {
                self.for_error = Some(tf("s_ml_menge_teilen", &[("0", Arg::from(amount_text(booked, piece)))]));
                Err(WrongAmount)
            }
        }
    }

    /// The booking went through: its lot gets a part given into the unit as soon as the Basetool has it.
    pub fn queue(&mut self, wish: For) {
        let since = self.link.as_ref().map_or(0.0, |l| l.sync.now());
        self.wishes.push(Wish {
            store: wish.store,
            name: wish.name,
            place: wish.place,
            quality: wish.quality,
            stolen: wish.stolen,
            unit: wish.unit,
            amount: wish.amount,
            since,
        });
        self.for_unit.clear();
        self.for_amount.clear();
        self.for_error = None;
        // The lot is made at the Basetool by the next pass.
        if let Some(link) = &self.link {
            link.sync.request_now();
        }
    }

    /// The booking waiting for its lot: when the Basetool has the lot and it holds the part, give it into the unit.
    fn fire_wish(&mut self, ctx: &egui::Context, link: &Link, rows: &[Row]) {
        if self.sending.is_some() {
            return;
        }
        let now = link.sync.now();
        let before = self.wishes.len();
        self.wishes.retain(|wish| now - wish.since <= WISH_WAIT_SECONDS);
        if self.wishes.len() < before {
            self.said = Some(t("s_ml_fuer_spaeter"));
        }
        // The oldest booking whose lot is there and holds it goes first; one send at a time.
        let ready = self.wishes.iter().enumerate().find_map(|(at, wish)| {
            let here = rows.iter().find(|r| {
                r.store == wish.store
                    && r.name.to_lowercase() == wish.name.to_lowercase()
                    && r.place.to_lowercase() == wish.place.to_lowercase()
                    && r.quality == wish.quality
                    && r.stolen == wish.stolen
            });
            let lot = here.and_then(|row| self.lot_of(row))?;
            // Without `split` the Basetool has not told yet how the lot divides.
            if !matches!(lot.get("split"), Some(Value::Object(_))) {
                return None;
            }
            let (personal, _) = split_of(lot);
            // The lot over there does not hold the booking yet: the next pass brings it.
            (personal + 1e-9 >= wish.amount).then(|| (at, share_op(lot, &wish.unit, wish.amount)))
        });
        if let Some((at, op)) = ready {
            self.wishes.remove(at);
            // A send that cannot start leaves the booking personal: the page says it can be given by hand later.
            if !self.dispatch(ctx, link, Kind::Share, op, true) {
                self.said = Some(t("s_ml_fuer_spaeter"));
            }
        }
    }

    fn lot_of(&self, row: &Row) -> Option<&Value> {
        let key = self.shares.get(&(row.store, row.index))?.lot.as_ref()?;
        self.snapshot.as_ref()?.lots.get(key)
    }

    /// The row as the chosen view shows it, or `None` where the view does not hold it.
    pub fn scoped(&self, row: &Row) -> Option<Row> {
        let Some(share) = self.shares.get(&(row.store, row.index)) else { return Some(row.clone()) };
        match self.scope {
            Scope::All => Some(row.clone()),
            Scope::Personal => (share.personal > 0.0).then(|| Row { amount: share.personal, ..row.clone() }),
            Scope::Shared => (share.shared > 0.0).then(|| Row { amount: share.shared, ..row.clone() }),
        }
    }

    /// Whether the row shows its whole amount: only then edit and delete count.
    pub fn whole(&self, row: &Row) -> bool {
        match self.scope {
            Scope::Shared => false,
            Scope::Personal => self.shares.get(&(row.store, row.index)).is_none_or(|s| s.shared <= 0.0),
            Scope::All => true,
        }
    }

    /// The buttons of a row: share when its lot has a personal part and there
    /// is a unit, take back when something lies in a pool.
    pub fn actions(&self, row: &Row) -> Vec<Kind> {
        let Some(lot) = self.lot_of(row) else { return Vec::new() };
        // Without `split` at the lot the Basetool knows neither op.
        if !matches!(lot.get("split"), Some(Value::Object(_))) {
            return Vec::new();
        }
        let (personal, pools) = split_of(lot);
        let mut out = Vec::new();
        if personal > 0.0 && !self.units.is_empty() && self.scope != Scope::Shared {
            out.push(Kind::Share);
        }
        if !pools.is_empty() && self.scope != Scope::Personal {
            out.push(Kind::Unshare);
        }
        out
    }

    /// The name of a unit: from the list, "left unit" for one the list no longer has.
    fn unit_name(&self, id: &str) -> String {
        match self.units.iter().find(|u| u.id == id) {
            Some(unit) => unit_label(unit),
            None => t(if self.units_fetched { "s_ml_einheit_weg" } else { "s_ml_einheit" }),
        }
    }

    /// The badges of a row: how much lies in a unit (all), in which unit (shared).
    pub fn badges(&self, scheme: &Scheme, row: &Row) -> Vec<(String, Color)> {
        let Some(share) = self.shares.get(&(row.store, row.index)) else { return Vec::new() };
        if share.shared <= 0.0 {
            return Vec::new();
        }
        let Some(lot) = self.lot_of(row) else { return Vec::new() };
        match self.scope {
            Scope::Shared => split_of(lot).1.iter().map(|(unit, _)| (self.unit_name(unit), scheme.accent)).collect(),
            Scope::All => {
                vec![(tf("s_ml_b_geteilt", &[("0", Arg::from(amount_text(share.shared, row.piece)))]), scheme.accent)]
            }
            Scope::Personal => Vec::new(),
        }
    }

    /// The words of the view switch and the place of the chosen one.
    pub fn scope_words(&self) -> [String; 3] {
        [t("s_ml_sicht_alle"), t("s_ml_sicht_persoenlich"), t("s_ml_sicht_gemeinsam")]
    }

    pub fn scope_index(&self) -> usize {
        match self.scope {
            Scope::All => 0,
            Scope::Personal => 1,
            Scope::Shared => 2,
        }
    }

    pub fn set_scope_index(&mut self, index: usize) {
        self.scope = match index {
            1 => Scope::Personal,
            2 => Scope::Shared,
            _ => Scope::All,
        };
    }

    /// Opens the form of a row (or closes it where it is open already).
    pub fn open(&mut self, row: &Row, kind: Kind) {
        if self.sending.is_some() || !self.actions(row).contains(&kind) {
            return;
        }
        let Some(lot) = self.shares.get(&(row.store, row.index)).and_then(|s| s.lot.clone()) else { return };
        if self.form.as_ref().is_some_and(|f| (f.store, f.index, f.kind) == (row.store, row.index, kind)) {
            self.form = None;
            return;
        }
        self.form = Some(Form {
            store: row.store,
            index: row.index,
            kind,
            name: row.name.clone(),
            lot,
            unit: String::new(),
            amount: String::new(),
            amount_for: String::new(),
            error: None,
        });
    }

    /// Whether the form of this row is open.
    pub fn open_for(&self, row: &Row) -> Option<Kind> {
        self.form.as_ref().filter(|f| (f.store, f.index) == (row.store, row.index)).map(|f| f.kind)
    }

    pub fn close(&mut self) {
        self.form = None;
    }

    /// The units a form may choose from and the most that can go: the personal part
    /// for sharing, the amount of the pool for taking back.
    fn choices(&self, form: &Form, lot: &Value) -> Vec<(String, String)> {
        match form.kind {
            Kind::Share => self.units.iter().map(|u| (u.id.clone(), unit_label(u))).collect(),
            Kind::Unshare => split_of(lot).1.iter().map(|(u, _)| (u.clone(), self.unit_name(u))).collect(),
        }
    }

    fn limit(form: &Form, lot: &Value) -> f64 {
        let (personal, pools) = split_of(lot);
        match form.kind {
            Kind::Share => personal,
            Kind::Unshare => pools.iter().find(|(u, _)| *u == form.unit).map_or(0.0, |(_, a)| *a),
        }
    }

    /// Takes the answer of a send that ended; the text for the page.
    pub fn poll(&mut self) -> Option<String> {
        if let Some(text) = self.said.take() {
            return Some(text);
        }
        let sending = self.sending.as_ref()?;
        let outcome = match sending.answer.try_recv() {
            Ok(outcome) => outcome,
            Err(mpsc::TryRecvError::Empty) => return None,
            Err(mpsc::TryRecvError::Disconnected) => {
                self.sending = None;
                return Some(t("s_sl_r_abgebrochen"));
            }
        };
        let (kind, booked) = (sending.kind, sending.booked);
        self.sending = None;
        // The new split arrives with the feed of the next pass.
        if let Some(link) = &self.link {
            link.sync.request_now();
        }
        let why = match outcome {
            Ok(()) => {
                self.form = None;
                return Some(t(if kind == Kind::Share { "s_ml_geteilt_ok" } else { "s_ml_zurueck_ok" }));
            }
            Err(Failure::Reason(reason)) => match reason_key(&reason) {
                Some(key) => t(key),
                None => tf("s_ml_r_anders", &[("0", Arg::from(reason))]),
            },
            Err(Failure::Api(error)) => {
                versekit_core::errors::record("my_stock_page.teilen", &error.describe());
                error_text(&error.code)
            }
        };
        // A booking that asked for it stays personal; the page says why.
        Some(if booked { tf("s_ml_fuer_abgelehnt", &[("0", Arg::from(why))]) } else { why })
    }

    /// The field "For" of the booking forms: only me, or a unit (a switch when there is one, a list otherwise)
    /// and the amount for it. Nothing without a connection.
    pub fn show_for(&mut self, ui: &mut Ui, scheme: &Scheme) {
        if !self.offered() {
            return;
        }
        if !self.units.iter().any(|u| u.id == self.for_unit) {
            self.for_unit.clear();
        }
        ui.add_space(6.0);
        ui.label(RichText::new(t("s_ml_fuer")).strong());
        let units = self.units.clone();
        ui.horizontal_wrapped(|row| {
            if let [only] = units.as_slice() {
                if widgets::radio(row, scheme, self.for_unit.is_empty(), &t("s_ml_nur_ich")).clicked() {
                    self.for_unit.clear();
                }
                if widgets::radio(row, scheme, self.for_unit == only.id, &unit_label(only)).clicked() {
                    self.for_unit = only.id.clone();
                }
            } else {
                let shown = units
                    .iter()
                    .find(|u| u.id == self.for_unit)
                    .map_or_else(|| t("s_ml_nur_ich"), unit_label);
                let combo = egui::ComboBox::from_id_salt("lager-fuer")
                    .selected_text(shown.as_str())
                    .width((row.available_width() / 2.0).clamp(120.0, 300.0))
                    .show_ui(row, |list| {
                        if list.selectable_label(self.for_unit.is_empty(), t("s_ml_nur_ich")).clicked() {
                            self.for_unit.clear();
                        }
                        for unit in &units {
                            if list.selectable_label(self.for_unit == unit.id, unit_label(unit)).clicked() {
                                self.for_unit = unit.id.clone();
                            }
                        }
                    });
                widgets::name_combo(&combo.response, &t("s_ml_fuer"), &shown);
            }
        });
        if !self.for_unit.is_empty() {
            let width = ui.available_width().clamp(80.0, 200.0);
            let hint = t("s_ml_fuer_menge");
            widgets::checked_field(ui, scheme, &mut self.for_amount, &hint, width, self.for_error.as_deref());
            ui.label(RichText::new(t("s_ml_fuer_menge_h")).small().color(color(scheme.sub)));
        }
    }

    /// Checks the amount and sends the op in a thread of its own.
    fn submit(&mut self, ctx: &egui::Context) {
        let Some(link) = self.link.clone() else { return };
        if self.sending.is_some() {
            return;
        }
        let Some(form) = self.form.as_ref() else { return };
        let Some(lot) = self.snapshot.as_ref().and_then(|s| s.lots.get(&form.lot)).cloned() else { return };
        let piece = lot.get("quantity").and_then(|q| q.get("unit")).and_then(Value::as_str) == Some(PIECE);
        let limit = Self::limit(form, &lot);
        let Some(value) = check_amount(&form.amount, limit, piece) else {
            let text = tf("s_ml_menge_teilen", &[("0", Arg::from(amount_text(limit, piece)))]);
            if let Some(form) = self.form.as_mut() {
                form.error = Some(text);
            }
            return;
        };
        let (kind, unit) = (form.kind, form.unit.clone());
        let op = match kind {
            Kind::Share => share_op(&lot, &unit, value),
            Kind::Unshare => unshare_op(&lot, &unit, value),
        };
        if self.dispatch(ctx, &link, kind, op, false)
            && let Some(form) = self.form.as_mut()
        {
            form.error = None;
        }
    }

    /// Sends an op in a thread of its own; `booked`: it belongs to a booking that asked for it, so a refusal
    /// says the booking stays personal. False when no thread could be started.
    fn dispatch(&mut self, ctx: &egui::Context, link: &Link, kind: Kind, op: Value, booked: bool) -> bool {
        let (send, answer) = mpsc::channel();
        let (ctx, link) = (ctx.clone(), link.clone());
        let started = std::thread::Builder::new().name("teilen".into()).spawn(move || {
            let _ = send.send(stock_split::send(&link.sync, &op));
            ctx.request_repaint();
        });
        if started.is_ok() {
            self.sending = Some(Sending { kind, answer, booked });
        } else {
            versekit_core::errors::record("my_stock_page.teilen", &"no thread");
        }
        started.is_ok()
    }

    /// The form above the list; the sending happens here.
    pub fn show_form(&mut self, ui: &mut Ui, scheme: &Scheme) {
        let Some(snapshot) = &self.snapshot else { return };
        let Some(form) = self.form.as_ref() else { return };
        let Some(lot) = snapshot.lots.get(&form.lot).cloned() else { return };
        let choices = self.choices(form, &lot);
        if choices.is_empty() {
            self.form = None;
            return;
        }
        let piece = lot.get("quantity").and_then(|q| q.get("unit")).and_then(Value::as_str) == Some(PIECE);
        let names: Vec<(String, String)> = choices.clone();
        let busy = self.sending.is_some();
        let mut go = false;
        let mut cancel = false;
        let mut picked: Option<String> = None;
        if let Some(form) = self.form.as_mut() {
            if !names.iter().any(|(id, _)| *id == form.unit) {
                form.unit = names[0].0.clone();
            }
            // Beginning with everything: the unit's own amount may change the proposal.
            if form.amount_for != form.unit {
                form.amount = field_text(Self::limit(form, &lot));
                form.amount_for = form.unit.clone();
            }
            let limit = Self::limit(form, &lot);
            let sharing = form.kind == Kind::Share;
            widgets::card(ui, scheme, |ui| {
                let title = tf(
                    if sharing { "s_ml_teilen_t" } else { "s_ml_zurueck_t" },
                    &[("0", Arg::from(form.name.clone()))],
                );
                widgets::card_title(ui, scheme, &title);
                ui.add_space(4.0);
                ui.horizontal_wrapped(|row| {
                    let shown = names.iter().find(|(id, _)| *id == form.unit).map(|(_, l)| l.clone()).unwrap_or_default();
                    let combo = egui::ComboBox::from_id_salt("lager-teilen-einheit")
                        .selected_text(shown.as_str())
                        .width((row.available_width() / 2.0).clamp(80.0, 260.0))
                        .show_ui(row, |list| {
                            for (id, label) in &names {
                                if list.selectable_label(*id == form.unit, label.as_str()).clicked() {
                                    picked = Some(id.clone());
                                }
                            }
                        });
                    widgets::name_combo(&combo.response, &t("s_ml_einheit"), &shown);
                    let width = (row.available_width() - 8.0).clamp(80.0, 140.0);
                    widgets::checked_field(row, scheme, &mut form.amount, &t("s_ml_menge_ph"), width, None);
                    row.label(
                        RichText::new(tf("s_ml_hoechstens", &[("0", Arg::from(amount_text(limit, piece)))]))
                            .small()
                            .color(color(scheme.sub)),
                    );
                });
                if let Some(error) = &form.error {
                    ui.label(RichText::new(error).color(color(scheme.red)));
                }
                ui.add_space(6.0);
                ui.horizontal_wrapped(|row| {
                    let word = t(if sharing { "s_ml_teilen_knopf" } else { "s_ml_zurueck_knopf" });
                    let button = row.add_enabled(!busy, widgets::Button::new(&word, ButtonKind::Strong, scheme));
                    go = button.clicked();
                    cancel = row.add(widgets::Button::new(&t("s_hl_abbrechen"), ButtonKind::Normal, scheme)).clicked();
                });
            });
        }
        if let Some(unit) = picked
            && let Some(form) = self.form.as_mut()
        {
            form.unit = unit;
            form.error = None;
        }
        if cancel {
            self.form = None;
        } else if go {
            let ctx = ui.ctx().clone();
            self.submit(&ctx);
        }
    }

    /// Test seam: fills the field "For".
    #[cfg(test)]
    pub(crate) fn set_for(&mut self, unit: &str, amount: &str) {
        self.for_unit = unit.to_string();
        self.for_amount = amount.to_string();
    }

    /// Test seam: a booking waits for its lot.
    #[cfg(test)]
    pub(crate) fn wish_pending(&self) -> bool {
        !self.wishes.is_empty()
    }

    /// Test seam: no send is under way.
    #[cfg(test)]
    pub(crate) fn idle(&self) -> bool {
        self.sending.is_none()
    }

    /// Test seam: how many bookings wait for their lot.
    #[cfg(test)]
    pub(crate) fn wishes_waiting(&self) -> usize {
        self.wishes.len()
    }

    /// Test seam: the booking has been waiting `seconds` longer.
    #[cfg(test)]
    pub(crate) fn age_wish(&mut self, seconds: f64) {
        for wish in &mut self.wishes {
            wish.since -= seconds;
        }
    }

    /// Test seam: the error at the field "For".
    #[cfg(test)]
    pub(crate) fn for_error(&self) -> Option<&str> {
        self.for_error.as_deref()
    }

    /// Test seam: the share of a row.
    #[cfg(test)]
    pub(crate) fn share_of(&self, row: &Row) -> Option<Share> {
        self.shares.get(&(row.store, row.index)).cloned()
    }

    /// Test seam: sets what is typed and sends.
    #[cfg(test)]
    pub(crate) fn type_and_send(&mut self, ctx: &egui::Context, unit: &str, amount: &str) {
        if let Some(form) = self.form.as_mut() {
            form.unit = unit.to_string();
            form.amount = amount.to_string();
            form.amount_for = unit.to_string();
        }
        self.submit(ctx);
    }

    /// Test seam: the form and its error text.
    #[cfg(test)]
    pub(crate) fn form_error(&self) -> Option<String> {
        self.form.as_ref().and_then(|f| f.error.clone())
    }

    #[cfg(test)]
    pub(crate) fn form_is_open(&self) -> bool {
        self.form.is_some()
    }

    #[cfg(test)]
    pub(crate) fn wait(&mut self) -> Option<String> {
        let start = std::time::Instant::now();
        while start.elapsed() < std::time::Duration::from_secs(10) {
            if let Some(text) = self.poll() {
                return Some(text);
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use versekit_core::basetool::exchange_double::ExchangeDouble;
    use versekit_core::basetool::scopes::ROUTE_STOCK;
    use versekit_core::jsonio::Value;
    use versekit_core::language::{self, Choice};
    use versekit_core::stock;

    use super::*;
    use crate::basetool_page::Wire;
    use crate::stock_page::{StockState, show_page};

    struct Bench {
        wire: Wire,
        double: Arc<ExchangeDouble>,
        state: StockState,
        ctx: egui::Context,
        dir: String,
    }

    impl Drop for Bench {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    fn titanium(amount: f64) -> Value {
        Value::Object(vec![
            ("material".into(), Value::str("Titanium")),
            ("menge".into(), Value::Float(amount)),
            ("qualitaet".into(), Value::int(600)),
            ("ort".into(), Value::str("Area18")),
        ])
    }

    fn bench(name: &str) -> Bench {
        let dir = std::env::temp_dir()
            .join(format!("versekit-teilen-{name}-{}", versekit_core::testdir::run_tag()))
            .to_string_lossy()
            .replace('\\', "/");
        let _ = std::fs::remove_dir_all(&dir);
        let double = Arc::new(ExchangeDouble::new());
        let wire = Wire::over_double(&double, &dir);
        assert!(wire.sync.set_setting(SETTING_STOCK, Value::Bool(true)));
        assert!(stock::save(&wire.sync.paths().app_file(stock::RAW_FILE), &[titanium(10.0)], "test"));
        double.set_units(&[("u1", "Alpha"), ("u2", "Beta")]);
        double.set_lot("Titanium", "Area18", 600, false, 10.0);
        wire.sync.pass_now().expect("pass");
        let mut state = StockState::sample_on(wire.sync.paths());
        state.connect(Some(Link::from_wire(&wire)));
        let ctx = egui::Context::default();
        crate::install(&ctx);
        let mut bench = Bench { wire, double, state, ctx, dir };
        bench.frame();
        bench
    }

    impl Bench {
        fn frame(&mut self) {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000.0, 1400.0))),
                ..Default::default()
            };
            let scheme = versekit_core::theme::current();
            let state = &mut self.state;
            let mut output = self.ctx.run_ui(input, |ui| show_page(ui, scheme, state));
            output.textures_delta.clear();
        }

        /// The sync fetches the new split, the page takes it.
        fn settle(&mut self) {
            self.wire.sync.pass_now().expect("pass");
            self.frame();
        }

        fn row(&self) -> Row {
            self.state.rows().into_iter().next().expect("a row")
        }
    }

    #[test]
    fn a_row_knows_its_shared_part_and_shares_and_takes_back_through_the_form() {
        let _language = crate::language_lock();
        language::set_language(Choice::De);
        let mut b = bench("teilen");
        let row = b.row();
        assert!(b.state.split.available());
        assert_eq!(b.state.split.actions(&row), vec![Kind::Share], "nothing shared yet, so nothing to take back");

        b.state.split.open(&row, Kind::Share);
        assert!(b.state.split.form_is_open());
        let ctx = b.ctx.clone();
        b.state.split.type_and_send(&ctx, "u1", "4");
        assert_eq!(b.state.split.wait().expect("answer"), t("s_ml_geteilt_ok"));
        assert!(!b.state.split.form_is_open(), "the form closes after a send that went through");
        assert_eq!(b.double.lots()[0].pools, vec![("u1".to_string(), 4.0)]);
        b.settle();

        let row = b.row();
        let share = b.state.split.share_of(&row).expect("share");
        assert_eq!((share.personal, share.shared), (6.0, 4.0));
        let scheme = versekit_core::theme::current();
        let said = tf("s_ml_b_geteilt", &[("0", Arg::from(amount_text(4.0, false)))]);
        assert_eq!(b.state.split.badges(scheme, &row)[0].0, said);
        assert_eq!(b.state.split.actions(&row), vec![Kind::Share, Kind::Unshare]);

        // The views: personal holds the rest, shared the part in the unit; only the whole row is edited.
        b.state.split.set_scope_index(1);
        assert_eq!(b.state.split.scoped(&row).expect("personal").amount, 6.0);
        assert!(!b.state.split.whole(&row), "a part of the lot is no row to edit or delete");
        b.state.split.set_scope_index(2);
        assert_eq!(b.state.split.scoped(&row).expect("shared").amount, 4.0);
        assert_eq!(b.state.split.badges(scheme, &row)[0].0, "Alpha");
        assert_eq!(b.state.split.actions(&row), vec![Kind::Unshare]);
        b.state.split.set_scope_index(0);
        assert!(b.state.split.whole(&row));

        // Taking back.
        b.state.split.open(&row, Kind::Unshare);
        let ctx = b.ctx.clone();
        b.state.split.type_and_send(&ctx, "u1", "4");
        assert_eq!(b.state.split.wait().expect("answer"), t("s_ml_zurueck_ok"));
        assert_eq!(b.double.lots()[0].pools, Vec::<(String, f64)>::new());
        b.settle();
        assert!(b.state.split.badges(scheme, &b.row()).is_empty());
        assert_eq!(b.state.split.actions(&b.row()), vec![Kind::Share]);
    }

    #[test]
    fn a_wrong_amount_sends_nothing_and_a_refusal_is_said_with_its_reason() {
        let _language = crate::language_lock();
        language::set_language(Choice::De);
        let mut b = bench("grund");
        let row = b.row();
        let sent = b.double.ops_sent(ROUTE_STOCK).len();
        b.state.split.open(&row, Kind::Share);
        let ctx = b.ctx.clone();
        // More than lies there: said at the form, nothing goes out.
        b.state.split.type_and_send(&ctx, "u1", "11");
        assert!(b.state.split.form_error().is_some());
        assert_eq!(b.double.ops_sent(ROUTE_STOCK).len(), sent);
        for wrong in ["0", "abc", "-1", ""] {
            b.state.split.type_and_send(&ctx, "u1", wrong);
            assert!(b.state.split.form_error().is_some(), "{wrong:?}");
        }
        // A unit the member is not in: the Basetool refuses, the page says why and keeps the form.
        b.state.split.type_and_send(&ctx, "u9", "1");
        assert_eq!(b.state.split.wait().expect("answer"), t("s_ml_r_kein_mitglied"));
        assert!(b.state.split.form_is_open());
        assert!(b.double.lots()[0].pools.is_empty());
    }

    #[test]
    fn a_form_closes_when_its_row_is_gone_and_the_view_goes_back_to_all_without_the_connection() {
        let _language = crate::language_lock();
        language::set_language(Choice::De);
        let mut b = bench("zu");
        let row = b.row();
        b.state.split.open(&row, Kind::Share);
        assert!(b.state.split.form_is_open());
        assert!(stock::save(&b.wire.sync.paths().app_file(stock::RAW_FILE), &[], "test"));
        b.state.follow_files();
        b.frame();
        assert!(!b.state.split.form_is_open(), "the row the form was opened for is gone");
        assert!(stock::save(&b.wire.sync.paths().app_file(stock::RAW_FILE), &[titanium(10.0)], "test"));
        b.state.follow_files();
        b.frame();
        b.state.split.set_scope_index(2);
        assert!(b.wire.sync.set_setting(SETTING_STOCK, Value::Bool(false)));
        b.frame();
        assert!(!b.state.split.available());
        assert_eq!(b.state.split.scope_index(), 0, "without the connection everything counts as personal");
        assert!(b.state.split.actions(&b.row()).is_empty());
    }

    impl Bench {
        /// Frames and passes until the booking has gone out or `rounds` are over.
        fn until_shared(&mut self, rounds: usize) -> Option<String> {
            // What the booking itself said does not count.
            self.state.take_message_text();
            for _ in 0..rounds {
                self.wire.sync.pass_now().expect("pass");
                self.frame();
                // Once the op is on its way the answer comes from its thread.
                let mut waits = 0;
                loop {
                    if let Some(text) = self.state.take_message_text() {
                        return Some(text);
                    }
                    if self.state.split.wish_pending() || waits > 250 {
                        break;
                    }
                    waits += 1;
                    std::thread::sleep(std::time::Duration::from_millis(20));
                    self.frame();
                }
            }
            None
        }

        /// The material entries as the file holds them.
        fn raw(&self) -> Vec<Value> {
            stock::load(&self.wire.sync.paths().app_file(stock::RAW_FILE), "test")
        }

        fn iron_in_the_unit(&self, place: &str) -> Vec<(String, f64)> {
            self.iron_in_the_unit_at(place, 420)
        }

        fn iron_in_the_unit_at(&self, place: &str, quality: i64) -> Vec<(String, f64)> {
            self.double
                .lots()
                .into_iter()
                .find(|l| l.name == "Iron" && l.place == place && l.quality == quality)
                .map(|l| l.pools)
                .unwrap_or_default()
        }
    }

    #[test]
    fn a_booking_for_a_unit_is_made_and_goes_into_the_unit_as_soon_as_the_basetool_has_it() {
        let _language = crate::language_lock();
        language::set_language(Choice::De);
        let mut b = bench("fuer");
        assert!(b.state.split.offered());
        // The whole booked amount: the booking is there at once, the unit gets it with the pass that fetches the lot.
        b.state.split.set_for("u1", "");
        b.state.book_raw("Iron", "3", "420", "Area18");
        assert_eq!(b.raw().len(), 2, "booked at once");
        assert!(b.state.split.wish_pending());
        b.frame();
        assert!(b.state.split.wish_pending(), "the Basetool has no lot yet");
        assert_eq!(b.until_shared(4), Some(t("s_ml_geteilt_ok")));
        assert_eq!(b.iron_in_the_unit("Area18"), vec![("u1".to_string(), 3.0)]);
        assert!(!b.state.split.wish_pending());
        // A part: the rest stays personal.
        b.state.split.set_for("u2", "1,5");
        b.state.book_raw("Iron", "4", "500", "Area18");
        assert_eq!(b.until_shared(4), Some(t("s_ml_geteilt_ok")));
        assert_eq!(b.iron_in_the_unit_at("Area18", 500), vec![("u2".to_string(), 1.5)]);
        // Only me: nothing is asked of the Basetool.
        let sent = b.double.ops_sent(versekit_core::basetool::scopes::ROUTE_STOCK).len();
        b.state.split.set_for("", "9");
        b.state.book_raw("Iron", "2", "600", "Area18");
        assert!(!b.state.split.wish_pending());
        b.wire.sync.pass_now().expect("pass");
        assert!(b.iron_in_the_unit_at("Area18", 600).is_empty());
        assert!(b.double.ops_sent(versekit_core::basetool::scopes::ROUTE_STOCK).len() > sent, "only the lot went out");
    }

    #[test]
    fn a_send_that_ended_without_an_answer_says_so_in_the_words_of_the_language_file() {
        let _language = crate::language_lock();
        for choice in [Choice::De, Choice::En] {
            language::set_language(choice);
            let mut split = Split::default();
            let (send, answer) = mpsc::channel();
            drop(send);
            split.sending = Some(Sending { kind: Kind::Share, answer, booked: false });
            let said = split.poll().expect("an answer");
            assert_eq!(said, t("s_sl_r_abgebrochen"));
            assert!(!said.contains("Disconnected") && !said.contains("{0}"), "{said}");
            assert!(split.sending.is_none());
        }
        language::set_language(Choice::De);
    }

    #[test]
    fn two_bookings_for_a_unit_before_the_first_pass_both_go_into_the_unit() {
        let _language = crate::language_lock();
        language::set_language(Choice::De);
        let mut b = bench("fuer-zwei");
        b.state.split.set_for("u1", "");
        b.state.book_raw("Iron", "3", "420", "Area18");
        b.state.split.set_for("u1", "");
        b.state.book_raw("Iron", "2", "500", "Area18");
        assert_eq!(b.state.split.wishes_waiting(), 2, "the second booking does not replace the first");
        for _ in 0..40 {
            if b.state.split.wishes_waiting() == 0 && b.state.split.idle() {
                break;
            }
            b.wire.sync.pass_now().expect("pass");
            b.frame();
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert_eq!(b.iron_in_the_unit_at("Area18", 420), vec![("u1".to_string(), 3.0)]);
        assert_eq!(b.iron_in_the_unit_at("Area18", 500), vec![("u1".to_string(), 2.0)]);
        assert_eq!(b.state.split.wishes_waiting(), 0);
    }

    #[test]
    fn a_wrong_amount_for_the_unit_books_nothing_and_a_refusal_leaves_the_booking_personal_with_its_reason() {
        let _language = crate::language_lock();
        language::set_language(Choice::De);
        let mut b = bench("fuer-falsch");
        b.state.split.set_for("u1", "9");
        b.state.book_raw("Iron", "2", "420", "Area18");
        assert!(b.state.split.for_error().is_some());
        assert_eq!(b.raw().len(), 1, "nothing booked");
        assert!(!b.state.split.wish_pending());
        // Goods for selling the same: a wrong amount for the unit books nothing.
        b.state.split.set_for("u1", "99");
        b.state.book_goods("Laranite", "5", "Area18");
        assert!(b.state.split.for_error().is_some());
        assert!(!b.state.split.wish_pending());
        b.state.split.set_for("", "");
        // The member is no longer in the unit at the Basetool: the booking stays, personal, and the page says why.
        b.state.split.set_for("u2", "");
        b.state.book_raw("Iron", "2", "420", "Area18");
        assert_eq!(b.raw().len(), 2);
        b.double.set_units(&[("u1", "Alpha")]);
        let said = b.until_shared(4).expect("an answer");
        assert_eq!(said, tf("s_ml_fuer_abgelehnt", &[("0", Arg::from(t("s_ml_r_kein_mitglied")))]));
        assert!(b.iron_in_the_unit("Area18").is_empty());
        assert_eq!(b.raw().len(), 2, "the booking is still there");
    }

    #[test]
    fn the_field_for_shows_only_with_a_connection_and_the_stock_synced_and_a_unit() {
        let mut b = bench("fuer-sichtbar");
        assert!(b.state.split.offered());
        assert!(b.wire.sync.set_setting(SETTING_STOCK, Value::Bool(false)));
        b.frame();
        assert!(!b.state.split.offered(), "area off");
        b.state.split.set_for("u1", "");
        b.state.book_raw("Iron", "1", "420", "Area18");
        assert!(!b.state.split.wish_pending(), "without the connection a choice leaves no wish");
        assert!(!StockState::default().split.offered());
    }

    impl Bench {
        /// The texts of one frame, lower case.
        fn texts(&mut self) -> Vec<String> {
            fn collect(shape: &egui::Shape, out: &mut Vec<String>) {
                match shape {
                    egui::Shape::Text(text) => out.push(text.galley.text().to_lowercase()),
                    egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| collect(s, out)),
                    _ => {}
                }
            }
            let mut texts = Vec::new();
            for _ in 0..2 {
                texts.clear();
                let input = egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000.0, 1800.0))),
                    ..Default::default()
                };
                let scheme = versekit_core::theme::current();
                let state = &mut self.state;
                let mut output = self.ctx.run_ui(input, |ui| show_page(ui, scheme, state));
                output.shapes.iter().for_each(|clipped| collect(&clipped.shape, &mut texts));
                output.textures_delta.clear();
            }
            texts
        }
    }

    #[test]
    fn the_booking_forms_show_the_field_for_as_a_switch_with_one_unit_and_a_list_with_more() {
        use versekit_core::language::{Lang, with_language};
        let mut b = bench("fuer-feld");
        // The form unfolds at once, or its lower part is cut off in the first frames.
        b.ctx.all_styles_mut(|style| style.animation_time = 0.0);
        b.state.open_forms();
        for language in [Lang::De, Lang::En] {
            with_language(language, || {
                let has = |seen: &[String], key: &str| seen.iter().any(|s| s == &t(key).to_lowercase());
                // Two units: a list that starts on "Only me", the amount appears with a unit.
                let seen = b.texts();
                assert!(has(&seen, "s_ml_fuer"), "{seen:?}");
                assert!(has(&seen, "s_ml_nur_ich"), "{seen:?}");
                assert!(!has(&seen, "s_ml_fuer_menge"), "no amount while it is only for me: {seen:?}");
                b.state.split.set_for("u1", "");
                let seen = b.texts();
                assert!(seen.iter().any(|s| s.contains(&t("s_ml_fuer_menge_h").to_lowercase())), "{seen:?}");
                b.state.split.set_for("", "");
            });
        }
        // One unit: a switch between "Only me" and the unit, no list.
        b.double.set_units(&[("u1", "Alpha")]);
        b.wire.sync.pass_now().expect("pass");
        let seen = b.texts();
        assert!(seen.iter().any(|s| s == "alpha"), "the unit is a choice of its own: {seen:?}");
    }

    #[test]
    fn a_booking_that_the_basetool_never_gets_says_so_after_the_waiting_time() {
        let _language = crate::language_lock();
        language::set_language(Choice::De);
        let mut b = bench("fuer-spaet");
        b.state.split.set_for("u1", "");
        b.state.book_raw("Iron", "2", "420", "Area18");
        // No pass runs: the lot never comes.
        b.state.take_message_text();
        b.state.split.age_wish(WISH_WAIT_SECONDS + 1.0);
        b.frame();
        assert_eq!(b.state.take_message_text(), Some(t("s_ml_fuer_spaeter")));
        assert!(!b.state.split.wish_pending());
    }

    #[test]
    fn a_page_without_a_connection_has_no_split() {
        let state = StockState::default();
        assert!(!state.split.available());
        let rows = state.rows();
        assert!(state.split.actions(&rows[0]).is_empty());
        assert!(state.split.whole(&rows[0]));
        assert_eq!(state.split.scoped(&rows[0]).map(|r| r.amount), Some(rows[0].amount));
    }
}
