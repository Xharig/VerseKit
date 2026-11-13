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

//! The page "Kartellbank": the accounts the member views with their 30-day curve and, on request, their history;
//! the own booking requests with raising, correcting and withdrawing; for bank staff the queue to look at, each
//! entry with its link into the Basetool. All of it in memory only (see `versekit_core::basetool::bank`).
//!
//! **A write keeps its key.** A raise, a correction or a withdrawal is held in the store of the bank with its
//! `Idempotency-Key` until a clear answer comes or the feed shows the request: sent again after a lost answer it
//! cannot make a second request. Closing the form keeps it; changing a field while its outcome is unclear asks
//! first; only the player's "Discard" lets it go unanswered.
//!
//! The streamer mode covers amounts, balances, handles and the free texts. A withheld name stays withheld.
//! Without the right to raise requests, or with twenty pending, there is no new request and no correcting or
//! withdrawing; without bank staff rights there is no queue tab. A deposit is not offered while the Basetool has
//! not confirmed how its account is named.

use std::sync::Arc;

use egui::{RichText, Ui};
use versekit_core::basetool::bank::{
    self, Account, Bank, BankStore, Correction, Held, History, Kind, QueueEntry, Raise, Request, Write, Written,
};
use versekit_core::basetool::connection::Clock;
use versekit_core::basetool::error::ApiError;
use versekit_core::basetool::scopes::SCOPES_BANK_REQUEST;
use versekit_core::basetool::sync::{Job, SyncHandle};
use versekit_core::formats;
use versekit_core::language::{self, Arg, t, tf};
use versekit_core::streamer;
use versekit_core::theme::Scheme;

use crate::color;
use crate::widgets::{self, ButtonKind};

/// The kinds a new request may have here. ⚠ No deposit: the contract does not say how a deposit names its account.
pub const OFFERED: [Kind; 2] = [Kind::Withdrawal, Kind::Transfer];

/// The application id requests raised through Verse-Kit carry in `client`.
const OWN_CLIENT: &str = "versekit";

/// What the page needs from the sync.
#[derive(Clone)]
pub struct Link {
    pub store: Arc<BankStore>,
    pub sync: SyncHandle,
    pub clock: Clock,
}

impl Link {
    pub fn from_wire(wire: &crate::basetool_page::Wire) -> Link {
        Link { store: wire.bank.clone(), sync: wire.sync.clone(), clock: versekit_core::basetool::connection::system_clock() }
    }
}

/// The tabs of the page.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tab {
    #[default]
    Accounts,
    Requests,
    Queue,
}

/// The form of a new request or of a correction.
#[derive(Clone, Debug, PartialEq)]
pub struct Form {
    pub kind: Kind,
    pub account: String,
    pub target: String,
    pub amount: String,
    pub note: String,
    pub justification: String,
    /// A correction: the request and the version last read.
    pub editing: Option<(String, i64)>,
    /// A correction of a request Verse-Kit did not raise: amount and target stay as they are, only note and
    /// justification change (another module's request refuses anything else).
    pub fixed: bool,
}

impl Form {
    fn new() -> Form {
        Form {
            kind: OFFERED[0],
            account: String::new(),
            target: String::new(),
            amount: String::new(),
            note: String::new(),
            justification: String::new(),
            editing: None,
            fixed: false,
        }
    }

    fn correcting(request: &Request) -> Form {
        Form {
            kind: Kind::from_code(&request.kind).unwrap_or(Kind::Withdrawal),
            account: request.account.as_ref().map(|a| a.id.clone()).unwrap_or_default(),
            target: request.target.as_ref().map(|a| a.id.clone()).unwrap_or_default(),
            amount: request.amount.to_string(),
            note: request.note.clone(),
            justification: request.justification.clone(),
            editing: Some((request.id.clone(), request.version)),
            fixed: request.client.as_deref() != Some(OWN_CLIENT),
        }
    }

    /// The form a raise held for its answer was sent from.
    fn of_raise(raise: &Raise) -> Form {
        Form {
            kind: raise.kind,
            account: raise.account.clone(),
            target: raise.target.clone().unwrap_or_default(),
            amount: raise.amount.to_string(),
            note: raise.note.clone(),
            justification: raise.justification.clone(),
            editing: None,
            fixed: false,
        }
    }

    /// The typed amount in whole aUEC, 0 when it does not read as one.
    pub fn amount(&self) -> i64 {
        parse_amount(&self.amount).unwrap_or(0)
    }

    pub fn raise(&self) -> Raise {
        Raise {
            kind: self.kind,
            account: self.account.clone(),
            target: (self.kind == Kind::Transfer).then(|| self.target.clone()),
            amount: self.amount(),
            note: self.note.clone(),
            justification: self.justification.clone(),
        }
    }

    /// The write the form makes.
    fn write(&self) -> Write {
        let raise = self.raise();
        match &self.editing {
            Some((id, version)) => Write::Correct(Correction {
                id: id.clone(),
                version: *version,
                amount: raise.amount,
                note: raise.note,
                justification: raise.justification,
                target: raise.target,
            }),
            None => Write::Raise(raise),
        }
    }
}

/// A whole amount of aUEC as typed: digits, a separator only between groups of exactly three digits (`15.000`,
/// `1,250,000`, `15 000`); one kind of separator. `12,5` is no amount.
pub fn parse_amount(text: &str) -> Option<i64> {
    let text = text.trim();
    let separators: Vec<char> = text.chars().filter(|c| matches!(c, '.' | ',' | ' ' | '\'')).collect();
    if text.is_empty() || separators.windows(2).any(|pair| pair[0] != pair[1]) {
        return None;
    }
    let groups: Vec<&str> = match separators.first() {
        Some(separator) => text.split(*separator).collect(),
        None => vec![text],
    };
    let digits = |group: &str| !group.is_empty() && group.chars().all(|c| c.is_ascii_digit());
    if !groups.iter().all(|g| digits(g)) {
        return None;
    }
    if groups.len() > 1 && (groups[0].len() > 3 || groups[1..].iter().any(|g| g.len() != 3)) {
        return None;
    }
    groups.concat().parse().ok()
}

/// The reading of the history of an account.
type HistoryJob = Job<Result<Arc<History>, ApiError>>;

/// What a write job is about.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Doing {
    Form,
    Withdraw,
}

/// A write on its way: the key it carries and what it is about.
struct WriteJob {
    key: String,
    doing: Doing,
    job: Job<Result<Written, ApiError>>,
}

/// What the page holds. The write held for its answer lives in the store of the bank, so a new page keeps it.
pub struct BankState {
    link: Option<Link>,
    pub tab: Tab,
    data: Option<Arc<Bank>>,
    seen: u64,
    /// The account whose history is open, the history, and the job that reads one (with its account).
    open: Option<String>,
    history: Option<Arc<History>>,
    history_job: Option<(String, HistoryJob)>,
    pub form: Option<Form>,
    write_job: Option<WriteJob>,
    /// The player changed a write of unclear outcome and was told so: the next send goes as a new one.
    confirm_change: bool,
    pub message: Option<(String, bool)>,
}

impl Default for BankState {
    /// The sample: a made-up bank.
    fn default() -> Self {
        BankState::new(None)
    }
}

fn sample() -> Bank {
    let account = |id: &str, no: &str, name: &str, balance: i64, line: &[i64], requestable: bool| Account {
        id: id.into(),
        no: no.into(),
        name: name.into(),
        kind: "ORG_UNIT".into(),
        balance,
        target: Some(2_000_000),
        delta_30d: balance - line[0],
        sparkline: line.to_vec(),
        requestable,
        ..Account::default()
    };
    Bank {
        accounts: vec![
            account("a1", "KB-0001", "KRT", 8_400_000, &[7_900_000, 8_100_000, 8_000_000, 8_400_000], false),
            account("a2", "KB-0042", "Staffel Mamba", 1_250_000, &[1_000_000, 1_100_000, 950_000, 1_250_000], true),
        ],
        requests: vec![Request {
            id: "r1".into(),
            status: "PENDING".into(),
            created_at: "2026-11-08T20:15:00Z".into(),
            kind: "WITHDRAWAL".into(),
            amount: 15_000,
            note: "Tanken".into(),
            account: Some(bank::AccountRef {
                id: "a2".into(),
                no: "KB-0042".into(),
                name: "Staffel Mamba".into(),
                ..Default::default()
            }),
            client: Some(OWN_CLIENT.into()),
            ..Request::default()
        }],
        open_cap: bank::OPEN_CAP,
        queue: None,
        not_staff: true,
    }
}

impl BankState {
    pub fn new(link: Option<Link>) -> BankState {
        let data = link.is_none().then(|| Arc::new(sample()));
        BankState {
            link,
            tab: Tab::Accounts,
            data,
            seen: 0,
            open: None,
            history: None,
            history_job: None,
            form: None,
            write_job: None,
            confirm_change: false,
            message: None,
        }
    }

    fn now(&self) -> f64 {
        self.link.as_ref().map_or(0.0, |l| (l.clock)())
    }

    /// Whether the member may raise, correct and withdraw requests (the right is granted).
    fn may_request(&self) -> bool {
        self.link.as_ref().is_none_or(|l| versekit_core::basetool::scopes::all_in(SCOPES_BANK_REQUEST, &l.sync.granted()))
    }

    /// The write held for its answer, if one is.
    fn held(&self) -> Option<Held> {
        self.link.as_ref().and_then(|l| l.store.held())
    }

    /// Until when the writes wait after a `Retry-After`.
    fn write_wait(&self) -> Option<f64> {
        self.link.as_ref().and_then(|l| l.store.write_wait(self.now()))
    }

    /// Takes in what the sync and the jobs brought.
    fn follow(&mut self) {
        let Some(link) = self.link.clone() else { return };
        let now = (link.clock)();
        let (generation, data) = link.store.current(now);
        if generation != self.seen {
            self.seen = generation;
            self.data = data;
        }
        if let Some((account, job)) = &mut self.history_job {
            let account = account.clone();
            let done = match job.poll() {
                Some(Ok(history)) => {
                    // Only the history of the account still open is shown; another one waits for its own reading.
                    if self.open.as_deref() == Some(account.as_str()) {
                        self.history = Some(history);
                    }
                    true
                }
                Some(Err(error)) => {
                    if self.open.as_deref() == Some(account.as_str()) {
                        self.message = Some((bank::error_text(&error.code), false));
                    }
                    true
                }
                None => job.lost(),
            };
            if done {
                self.history_job = None;
            }
        }
        // The account opened while another one was read gets its own reading.
        if let Some(open) = self.open.clone()
            && self.history.is_none()
            && self.history_job.is_none()
        {
            self.read_history(&link, &open);
        }
        if let Some(write) = &mut self.write_job {
            let answer = match write.job.poll() {
                Some(answer) => Some(answer),
                None if write.job.lost() => Some(Err(ApiError::network("lost"))),
                None => None,
            };
            if let Some(answer) = answer {
                let (key, doing) = (write.key.clone(), write.doing);
                self.write_job = None;
                self.answered(&link, &key, doing, answer, now);
            }
        }
    }

    /// The answer of a write: a clear one lets its key go, an unclear one keeps it; a wait asked for is kept.
    fn answered(&mut self, link: &Link, key: &str, doing: Doing, answer: Result<Written, ApiError>, now: f64) {
        match answer {
            Ok(_) => {
                link.store.answered(key);
                self.confirm_change = false;
                if doing == Doing::Form {
                    self.form = None;
                }
                self.message = Some((t("s_bank_gesendet"), true));
                link.sync.request_now();
            }
            Err(error) => {
                if let Some(wait) = error.retry_after {
                    link.store.set_write_wait(now + wait as f64);
                }
                let mut text = bank::error_text(&error.code);
                if bank::settled(&error) {
                    link.store.answered(key);
                } else {
                    link.store.mark_unclear(key);
                    text = format!("{text} {}", t("s_bank_unklar"));
                }
                if error.code == "VERSION_CONFLICT" {
                    link.sync.request_now();
                }
                self.message = Some((text, false));
            }
        }
    }

    fn read_history(&mut self, link: &Link, account: &str) {
        let now = (link.clock)();
        self.history = link.store.history(account, now);
        if self.history.is_none() && self.history_job.is_none() {
            let (store, id) = (link.store.clone(), account.to_string());
            let job = link.sync.job(move |conn| store.read_history(conn, &id, now));
            self.history_job = Some((account.to_string(), job));
        }
    }

    fn open_history(&mut self, account: &str) {
        if self.open.as_deref() == Some(account) {
            self.open = None;
            self.history = None;
            return;
        }
        self.open = Some(account.to_string());
        self.history = None;
        if let Some(link) = self.link.clone() {
            self.read_history(&link, account);
        }
    }

    /// Opens the form of a new request; a raise of unclear outcome comes back as it was sent.
    fn open_new(&mut self) {
        self.message = None;
        self.form = Some(match self.held().map(|h| h.write) {
            Some(Write::Raise(raise)) => Form::of_raise(&raise),
            _ => Form::new(),
        });
    }

    /// Starts a write under the key it holds: the same write as the one held goes under its key; another one while
    /// the held one is unclear is refused once with a warning.
    fn start(&mut self, write: Write, doing: Doing) {
        let Some(link) = self.link.clone() else {
            self.message = Some((t("s_sle_beispiel_browser"), false));
            return;
        };
        if self.write_job.is_some() {
            return;
        }
        if let Some(until) = self.write_wait() {
            self.message = Some((wait_text(until), false));
            return;
        }
        let held = match link.store.held() {
            Some(held) if held.write == write => held,
            Some(held) if held.unclear && !self.confirm_change => {
                let key = if matches!(held.write, Write::Raise(_)) { "s_bank_unklar_geaendert" } else { "s_bank_erst_klaeren" };
                self.message = Some((t(key), false));
                self.confirm_change = doing == Doing::Form && matches!(held.write, Write::Raise(_));
                return;
            }
            _ => {
                let known = self.data.as_ref().map(|d| d.requests.iter().map(|r| r.id.clone()).collect()).unwrap_or_default();
                Held::new(write, known)
            }
        };
        self.confirm_change = false;
        link.store.set_held(Some(held.clone()));
        self.message = None;
        let key = held.key.clone();
        let job = link.sync.job(move |conn| bank::send(conn, &held));
        self.write_job = Some(WriteJob { key, doing, job });
    }

    /// Sends the form: a raise or a correction, each under the key it holds.
    pub fn send(&mut self) {
        let Some(form) = self.form.clone() else { return };
        if parse_amount(&form.amount).is_none() {
            self.message = Some((t("s_bank_f_betrag"), false));
            return;
        }
        if let Some(problem) = bank::check(&form.raise()) {
            self.message = Some((t(problem), false));
            return;
        }
        self.start(form.write(), Doing::Form);
    }

    fn withdraw(&mut self, request: &Request) {
        self.start(Write::Cancel { id: request.id.clone(), version: request.version }, Doing::Withdraw);
    }

    /// The player lets a write of unclear outcome go: it may have reached the Basetool.
    fn discard(&mut self) {
        if let Some(link) = &self.link {
            link.store.set_held(None);
        }
        self.confirm_change = false;
        self.message = None;
    }
}

fn wait_text(until: f64) -> String {
    tf("s_bank_warten", &[("0", Arg::from(formats::date_time(language::current(), until)))])
}

fn sub(scheme: &Scheme, text: impl Into<String>) -> RichText {
    RichText::new(text.into()).small().color(color(scheme.sub))
}

/// Money as shown: hidden in streamer mode.
pub fn money(value: i64) -> String {
    if streamer::active() {
        return streamer::placeholder();
    }
    formats::money(language::current(), value as f64)
}

/// A handle as shown: a withheld one says so, the streamer mode covers it.
pub fn handle(name: Option<&str>) -> String {
    match name {
        Some(name) if !name.is_empty() => {
            if streamer::active() {
                streamer::placeholder()
            } else {
                streamer::private(name.to_string())
            }
        }
        _ => t("s_bank_name_verborgen"),
    }
}

/// A free text (note, justification, reason) as shown: the streamer mode covers it.
pub fn free_text(text: &str) -> String {
    if streamer::active() && !text.is_empty() { streamer::placeholder() } else { streamer::scrub(text) }
}

pub fn kind_text(code: &str) -> String {
    match code {
        "DEPOSIT" => t("s_bank_einzahlung"),
        "WITHDRAWAL" => t("s_bank_auszahlung"),
        "TRANSFER" => t("s_bank_ueberweisung"),
        "HOLDER_TRANSFER" => t("s_bank_halterwechsel"),
        "WIPE_RESET" => t("s_bank_ruecksetzung"),
        "REVERSAL" => t("s_bank_storno"),
        other => tf("s_bank_sonstiges", &[("0", Arg::from(other))]),
    }
}

pub fn status_text(code: &str) -> String {
    match code {
        "PENDING" => t("s_bank_offen"),
        "CONFIRMED" => t("s_bank_bestaetigt"),
        "REJECTED" => t("s_bank_abgelehnt"),
        "CANCELLED" => t("s_bank_zurueckgezogen"),
        other => tf("s_bank_sonstiges", &[("0", Arg::from(other))]),
    }
}

/// A small line of balances, drawn without numbers.
fn curve(ui: &mut Ui, scheme: &Scheme, points: &[i64], width: f32, height: f32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::hover());
    if points.len() < 2 {
        return;
    }
    let (low, high) = (*points.iter().min().unwrap_or(&0), *points.iter().max().unwrap_or(&0));
    let span = (high - low).max(1) as f32;
    let step = rect.width() / (points.len() - 1) as f32;
    let line: Vec<egui::Pos2> = points
        .iter()
        .enumerate()
        .map(|(i, p)| egui::pos2(rect.left() + step * i as f32, rect.bottom() - (*p - low) as f32 / span * rect.height()))
        .collect();
    ui.painter().add(egui::Shape::line(line, egui::Stroke::new(1.5, color(scheme.accent))));
}

/// The page.
pub fn show_page(ui: &mut Ui, scheme: &Scheme, state: &mut BankState) {
    state.follow();
    widgets::reading(ui, sub(scheme, t("s_bank_lead")));
    ui.add_space(6.0);
    if state.link.as_ref().is_some_and(|l| l.store.disabled()) {
        widgets::reading(ui, RichText::new(t("s_btv_f_bank_aus")).color(color(scheme.gold)));
    }
    let Some(data) = state.data.clone() else {
        widgets::empty_state(ui, scheme, "landmark", &t("s_bank_noch_nichts"));
        return;
    };
    let mut tabs = vec![t("s_bank_konten"), t("s_bank_antraege")];
    let queue = data.queue.as_ref().filter(|_| !data.not_staff);
    if queue.is_some() {
        tabs.push(t("s_bank_warteschlange"));
    } else if state.tab == Tab::Queue {
        state.tab = Tab::Accounts;
    }
    let names: Vec<&str> = tabs.iter().map(String::as_str).collect();
    let mut chosen = match state.tab {
        Tab::Accounts => 0,
        Tab::Requests => 1,
        Tab::Queue => 2,
    };
    if widgets::tab_bar(ui, scheme, &names, &mut chosen) {
        state.tab = [Tab::Accounts, Tab::Requests, Tab::Queue][chosen.min(2)];
    }
    ui.add_space(6.0);
    if let Some((text, good)) = &state.message {
        let tone = if *good { scheme.accent } else { scheme.red_text() };
        widgets::reading(ui, RichText::new(text).color(color(tone)));
    }
    match state.tab {
        Tab::Accounts => accounts(ui, scheme, state, &data),
        Tab::Requests => requests(ui, scheme, state, &data),
        Tab::Queue => {
            if let Some(queue) = queue {
                queue_rows(ui, scheme, &queue.open, &queue.to_approve);
            }
        }
    }
}

fn accounts(ui: &mut Ui, scheme: &Scheme, state: &mut BankState, data: &Bank) {
    if data.accounts.is_empty() {
        widgets::empty_state(ui, scheme, "landmark", &t("s_bank_keine_konten"));
        return;
    }
    let mut clicked = None;
    widgets::list_frame(ui, scheme, |list| {
        list.set_width(list.available_width());
        for account in &data.accounts {
            list.horizontal(|row| {
                row.label(RichText::new(account.label()).color(color(scheme.fg)));
                row.label(sub(scheme, money(account.balance)));
                if account.delta_30d != 0 {
                    let sign = if account.delta_30d > 0 && !streamer::active() { "+" } else { "" };
                    let delta = format!("{sign}{}", money(account.delta_30d));
                    row.label(sub(scheme, tf("s_bank_30_tage", &[("0", Arg::from(delta))])));
                }
                // Button and curve stand at the right edge, so they line up from account to account.
                row.with_layout(egui::Layout::right_to_left(egui::Align::Center), |right| {
                    let open = state.open.as_deref() == Some(&account.id);
                    let label = if open { t("s_bank_verlauf_zu") } else { t("s_bank_verlauf") };
                    let button = widgets::Button::new(&label, ButtonKind::Normal, scheme).icon("chart-column");
                    if right.add(button).clicked() {
                        clicked = Some(account.id.clone());
                    }
                    curve(right, scheme, &account.sparkline, 90.0, 18.0);
                });
            });
            if state.open.as_deref() == Some(&account.id) {
                let reading = state.history_job.as_ref().is_some_and(|(id, _)| *id == account.id);
                history(list, scheme, state.history.as_deref(), reading);
            }
        }
    });
    if let Some(account) = clicked {
        state.open_history(&account);
    }
}

fn history(ui: &mut Ui, scheme: &Scheme, history: Option<&History>, reading: bool) {
    let Some(history) = history else {
        if reading {
            ui.horizontal(|row| {
                row.add(egui::Spinner::new().color(color(scheme.accent)));
                row.label(sub(scheme, t("s_bank_liest")));
            });
        }
        return;
    };
    let points: Vec<i64> = history.series.iter().map(|(_, b)| *b).collect();
    ui.label(sub(scheme, tf("s_bank_kurve", &[("0", Arg::Int(points.len() as i64))])));
    curve(ui, scheme, &points, ui.available_width().min(520.0), 80.0);
    if history.bookings.is_empty() {
        ui.label(sub(scheme, t("s_bank_keine_buchungen")));
    }
    for booking in &history.bookings {
        let mut line = format!(
            "{}  ·  {}  ·  {}",
            booking.created_at.get(..10).unwrap_or(""),
            kind_text(&booking.kind),
            money(booking.amount)
        );
        if let Some(counter) = &booking.counter_account {
            line.push_str(&format!("  ·  {counter}"));
        }
        line.push_str(&format!("  ·  {}", handle(booking.counterparty.as_deref())));
        if !booking.note.is_empty() {
            line.push_str(&format!("  ·  {}", free_text(&booking.note)));
        }
        ui.label(sub(scheme, line));
    }
}

fn requests(ui: &mut Ui, scheme: &Scheme, state: &mut BankState, data: &Bank) {
    let may = state.may_request();
    let held = state.held().filter(|h| h.unclear);
    if let Some(held) = &held {
        widgets::reading(ui, RichText::new(t("s_bank_unklar")).color(color(scheme.gold)));
        let resend = matches!(held.write, Write::Cancel { .. }) && state.form.is_none();
        ui.horizontal(|row| {
            if resend && row.add(widgets::Button::new(&t("s_bank_erneut"), ButtonKind::Normal, scheme).icon("send")).clicked() {
                let write = held.write.clone();
                state.start(write, Doing::Withdraw);
            }
            if row.add(widgets::Button::new(&t("s_bank_verwerfen"), ButtonKind::Danger, scheme).icon("trash-2")).clicked() {
                state.discard();
            }
        });
    }
    if data.cap_reached() {
        widgets::reading(ui, sub(scheme, t("s_bank_f_deckel")));
    } else if !may {
        widgets::reading(ui, sub(scheme, t("s_bank_f_recht")));
    } else if state.form.is_none()
        && ui.add(widgets::Button::new(&t("s_bank_neu"), ButtonKind::Strong, scheme).icon("plus")).clicked()
    {
        state.open_new();
    }
    if state.form.is_some() && may {
        form(ui, scheme, state, data);
    }
    ui.add_space(6.0);
    if data.requests.is_empty() {
        widgets::empty_state(ui, scheme, "landmark", &t("s_bank_keine_antraege"));
        return;
    }
    let mut correct = None;
    let mut withdraw = None;
    widgets::list_frame(ui, scheme, |list| {
        list.set_width(list.available_width());
        for request in &data.requests {
            list.horizontal_wrapped(|row| {
                row.label(RichText::new(kind_text(&request.kind)).color(color(scheme.fg)));
                row.label(sub(scheme, money(request.amount)));
                if let Some(account) = &request.account {
                    row.label(sub(scheme, account.label()));
                }
                if let Some(target) = &request.target {
                    row.label(sub(scheme, format!("→ {}", target.label())));
                }
                row.label(sub(scheme, status_text(&request.status)));
                if request.needs_approval && !request.approved && request.pending() {
                    row.label(sub(scheme, t("s_bank_freigabe_noetig")));
                }
                if may
                    && request.correctable()
                    && row
                        .add(widgets::Button::new(&t("s_bank_korrigieren"), ButtonKind::Normal, scheme).icon("pencil"))
                        .clicked()
                {
                    correct = Some(request.clone());
                }
                if may
                    && request.pending()
                    && row
                        .add(widgets::Button::new(&t("s_bank_zurueckziehen"), ButtonKind::Normal, scheme).icon("undo-2"))
                        .clicked()
                {
                    withdraw = Some(request.clone());
                }
            });
            if !request.note.is_empty() {
                list.label(sub(scheme, free_text(&request.note)));
            }
            if let Some(reason) = &request.reject_reason {
                list.label(sub(scheme, tf("s_bank_grund", &[("0", Arg::from(free_text(reason)))])));
            }
        }
    });
    if let Some(request) = correct {
        state.form = Some(Form::correcting(&request));
        state.message = None;
    }
    if let Some(request) = withdraw {
        state.withdraw(&request);
    }
}

fn form(ui: &mut Ui, scheme: &Scheme, state: &mut BankState, data: &Bank) {
    let wait = state.write_wait();
    let busy = state.write_job.is_some();
    let Some(form) = state.form.as_mut() else { return };
    let editing = form.editing.is_some();
    widgets::card_title(ui, scheme, &t(if editing { "s_bank_korrektur" } else { "s_bank_neu" }));
    if form.fixed {
        widgets::reading(ui, sub(scheme, t("s_bank_nur_text")));
    }
    if !editing {
        ui.horizontal(|row| {
            for kind in OFFERED {
                let text = kind_text(kind.code());
                if widgets::tab(row, scheme, form.kind == kind, &text).clicked() {
                    form.kind = kind;
                }
            }
        });
        // A withdrawal or transfer only from an account the member may ask from.
        let sources: Vec<&Account> = data.accounts.iter().filter(|a| a.requestable).collect();
        let shown = sources.iter().find(|a| a.id == form.account).map(|a| a.label()).unwrap_or_default();
        let name = t("s_bank_konto");
        ui.label(sub(scheme, &name));
        crate::settings::choice_box(ui, "bank-konto", &name, &shown, |list| {
            for account in &sources {
                list.selectable_value(&mut form.account, account.id.clone(), account.label());
            }
        });
    }
    if form.kind == Kind::Transfer && !form.fixed {
        let shown = data.accounts.iter().find(|a| a.id == form.target).map(|a| a.label()).unwrap_or_default();
        let name = t("s_bank_ziel");
        ui.label(sub(scheme, &name));
        crate::settings::choice_box(ui, "bank-ziel", &name, &shown, |list| {
            for account in data.accounts.iter().filter(|a| a.id != form.account) {
                list.selectable_value(&mut form.target, account.id.clone(), account.label());
            }
        });
    }
    let covered = streamer::active();
    if !form.fixed {
        ui.add(widgets::text_field(&mut form.amount, &t("s_bank_betrag"), scheme).password(covered));
    }
    ui.add(widgets::text_field(&mut form.note, &t("s_bank_notiz"), scheme).password(covered));
    if form.kind != Kind::Deposit {
        ui.add(widgets::text_field(&mut form.justification, &t("s_bank_begruendung"), scheme).password(covered));
    }
    let mut send = false;
    let mut close = false;
    ui.horizontal(|row| {
        let label = t(if editing { "s_bank_korrigieren" } else { "s_bank_senden" });
        let blocked = match wait {
            Some(until) => Some(wait_text(until)),
            None => busy.then(|| t("s_bank_sendet")),
        };
        if row
            .add(widgets::Button::new(&label, ButtonKind::Strong, scheme).icon("send").blocked(blocked.as_deref()))
            .clicked()
        {
            send = true;
        }
        if row.add(widgets::Button::new(&t("s_bank_abbrechen"), ButtonKind::Normal, scheme).icon("x")).clicked() {
            close = true;
        }
    });
    if send {
        state.send();
    }
    if close {
        // Closing keeps a write held for its answer: "Neuer Antrag" brings it back.
        state.form = None;
        state.confirm_change = false;
    }
}

fn queue_rows(ui: &mut Ui, scheme: &Scheme, open: &[QueueEntry], to_approve: &[QueueEntry]) {
    for (title, entries) in [("s_bank_zu_bearbeiten", open), ("s_bank_freizugeben", to_approve)] {
        widgets::card_title(ui, scheme, &t(title));
        if entries.is_empty() {
            ui.label(sub(scheme, t("s_bank_nichts_offen")));
            continue;
        }
        widgets::list_frame(ui, scheme, |list| {
            list.set_width(list.available_width());
            for entry in entries {
                list.horizontal_wrapped(|row| {
                    row.label(RichText::new(kind_text(&entry.kind)).color(color(scheme.fg)));
                    row.label(sub(scheme, money(entry.amount)));
                    if let Some(account) = &entry.account {
                        row.label(sub(scheme, account.label()));
                    }
                    row.label(sub(scheme, handle(entry.requester.as_deref())));
                    row.label(sub(scheme, tf("s_bank_alter", &[("0", Arg::Int(entry.age_hours))])));
                    if !entry.link.is_empty()
                        && row
                            .add(
                                widgets::Button::new(&t("s_bank_im_basetool"), ButtonKind::Normal, scheme)
                                    .icon("external-link"),
                            )
                            .clicked()
                    {
                        versekit_core::browser::open_in_browser(&entry.link);
                    }
                });
                if !entry.justification.is_empty() {
                    list.label(sub(scheme, free_text(&entry.justification)));
                }
            }
        });
    }
}

#[cfg(test)]
mod tests;
