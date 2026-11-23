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

//! The page "Finanzen": the account (the balance the player entered, every booking of the Game.log since, the
//! statement with the balance after each line) and the evaluation of what came in and went out in aUEC in a
//! period (today, the session, seven days, the month, days of one's own), set against the period before, by the
//! source the Game.log shows it from.
//!
//! ⚠ **Nothing is guessed.** The sums are the ones the log shows (`versekit_core::finance`). A sale from the ship
//! has only its request in the log, with the exact amount: it counts in full and is marked. A completed contract
//! is counted and has an amount only where the catalog states one for its title (the one VerseKit writes into the
//! text of the contract); it is then income and a line of the statement, marked `ca.` where variants of the title
//! pay differently (the lowest amount). What the log does not see shows up as
//! the check line when the player enters the balance again.
//!
//! The page shows the saved journal at once; the running `Game.log` is read in the background when the page is
//! opened and now and then while it stays open. The streamer mode covers the amounts.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use egui::{RichText, Ui};
use versekit_core::finance::{self, Account, Ask, Before, Booking, Count, Entry, Period, Totals, Window};
use versekit_core::contract_rewards::Reward;
use versekit_core::formats;
use versekit_core::jsonio::Value;
use versekit_core::language::{self, Arg, t, tf};
use versekit_core::logsource::LogSource;
use versekit_core::paths::{Host, Paths};
use versekit_core::stock_journal::names::{Items, Places};
use versekit_core::stock_journal::{self, Session};
use versekit_core::theme::Scheme;
use versekit_core::{contract_rewards, mission_log, playtime, streamer, writeguard};

use crate::layout::GAP;
use crate::statistics_page::{
    Cell, TILE_MIN, completed_contracts, moment, notes, section, signed_money, small, table, tile, tiles,
};
use crate::widgets::{self, ButtonKind};

/// How often the open page reads the running log again (seconds).
const REFRESH_SECONDS: f64 = 300.0;
/// How many lines of the statement show at first, and how many more a click adds.
const STATEMENT_PAGE: usize = 15;
/// The setting that keeps the chosen period.
pub const PERIOD_SETTING: &str = "finanzen_zeitraum";
/// What stands for a value that is not there.
const NONE: &str = "-";

/// What the page works on: the files of the data folder read at one moment.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Data {
    /// The moment the files were read (seconds since 1970).
    pub now: i64,
    pub sessions: Vec<Session>,
    /// The contracts completed, by the moment they ended, with the reward the catalog states for them.
    pub contracts: Vec<finance::Contract>,
    /// The sessions of the game as (from, to).
    pub spans: Vec<(i64, i64)>,
    pub counts: Vec<Count>,
    /// The account: the statement from the first balance entered.
    pub account: Account,
    /// The reading of the stock lines is switched off.
    pub journal_off: bool,
}

impl Data {
    /// Reads the files of the data folder.
    pub fn load(paths: &Paths) -> Data {
        let journal_off = !stock_journal::enabled(paths);
        let sessions = if journal_off { Vec::new() } else { stock_journal::sessions(&stock_journal::load(paths)) };
        let contracts = contracts_of(paths);
        let mut spans = playtime::spans(paths);
        spans.extend(playtime::running_span(paths));
        let counts = finance::counts(paths);
        let (items, places) = (Items::load(paths), Places::load(paths));
        let account = account_of(&counts, &sessions, &contracts, &items, &places);
        Data { now: now_seconds(), sessions, contracts, spans, counts, account, journal_off }
    }

    /// Made-up numbers for the sample page: nothing of the user's.
    pub fn sample() -> Data {
        use versekit_core::stock_journal::{Deal, Proof, Transfer};
        let now = SAMPLE_NOW;
        let deal = |ago: i64, price: f64, proof: Proof, goods: bool| Deal {
            time: now - ago,
            class: if goods { String::new() } else { "sample_item".into() },
            guid: "sample".into(),
            quantity: 1,
            price_total: price,
            place: None,
            shop: "SCShop_Sample".into(),
            proof: if goods { proof } else { Proof::Answer },
        };
        let day = 86_400;
        let session = |ago: i64, sales: Vec<Deal>, purchases: Vec<Deal>, transfers: Vec<Transfer>| Session {
            start: now - ago,
            purchases,
            sales,
            movements: Vec::new(),
            transfers,
        };
        let sessions = vec![
            session(
                9 * day,
                vec![deal(9 * day - 600, 210_000.0, Proof::BoxGone, true)],
                vec![deal(9 * day - 300, 18_000.0, Proof::Answer, false)],
                Vec::new(),
            ),
            session(
                2 * day + 7200,
                vec![
                    deal(2 * day + 5400, 640_000.0, Proof::BoxGone, true),
                    deal(2 * day + 5000, 88_000.0, Proof::Answer, false),
                ],
                vec![deal(2 * day + 4800, 24_500.0, Proof::Answer, false)],
                vec![Transfer { time: now - 2 * day + 600, incoming: true, amount: 100_000 }],
            ),
            session(
                6000,
                vec![
                    deal(5000, 395_000.0, Proof::BoxGone, true),
                    deal(4000, 132_000.0, Proof::Requested, true),
                    deal(3500, 21_000.0, Proof::Answer, false),
                ],
                vec![deal(4500, 9_800.0, Proof::Answer, false)],
                vec![Transfer { time: now - 3000, incoming: false, amount: 50_000 }],
            ),
        ];
        let spans = vec![
            (now - 9 * day - 3600, now - 9 * day + 3600),
            (now - 2 * day - 3600, now - 2 * day + 7200),
            (now - 6000, now),
        ];
        let counts = vec![
            Count { time: now - 9 * day - 1200, balance: 1_100_000, logged: None, unseen: None, requested: 0.0 },
            Count {
                time: now - 2 * day,
                balance: 1_950_000,
                logged: Some(401_000.0),
                unseen: Some(449_000.0),
                requested: 0.0,
            },
        ];
        let items = Items::from_parts(
            [("item_Namesample_item".to_string(), t("s_fi_b_muster"))],
            Vec::new(),
            Vec::new(),
        )
        .with_goods([("sample".to_string(), "Gold".to_string())]);
        let contract = |ago: i64, name: &str, reward: Option<Reward>, cost: Option<Reward>| finance::Contract {
            time: now - ago,
            name: name.to_string(),
            reward,
            taken: now - ago - 1800,
            cost,
        };
        let contracts = vec![
            contract(2 * day - 3600, &t("s_fi_muster_auftrag_a"), Some(Reward { amount: 25_000, approx: false }), None),
            contract(2 * day - 3700, &t("s_fi_muster_auftrag_b"), Some(Reward { amount: 18_000, approx: true }), None),
            contract(2500, &t("s_fi_muster_auftrag_c"), None, Some(Reward { amount: 9_000, approx: false })),
        ];
        let account = account_of(&counts, &sessions, &contracts, &items, &Places::default());
        Data {
            now,
            sessions,
            contracts,
            spans,
            counts,
            account,
            journal_off: false,
        }
    }
}

/// The account of the counts and the journal, with the names of the goods, items and places.
fn account_of(
    counts: &[Count],
    sessions: &[Session],
    contracts: &[finance::Contract],
    items: &Items,
    places: &Places,
) -> Account {
    let item = |class: &str, guid: &str| items.item(class, guid).name().map(str::to_string);
    let place = |number: &str| places.place(number).name().map(str::to_string);
    finance::account_with(counts, sessions, contracts, &finance::Namer { item: &item, place: &place })
}

/// The contracts completed in the mission log, each with the reward the catalog states for its title.
fn contracts_of(paths: &Paths) -> Vec<finance::Contract> {
    completed_contracts(&mission_log::load(paths), &contract_rewards::Rewards::load_kept(paths))
}

/// The moment of the sample page: 2026-11-21 12:00 UTC.
const SAMPLE_NOW: i64 = 1_795_262_400;

fn now_seconds() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64)
}

/// What a run does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Job {
    /// Reads the running log.
    Refresh,
    /// Reads the running log, then enters this balance as a count against what the log shows.
    Count(i64),
    /// Takes the newest count back.
    Undo,
}

#[derive(Default)]
struct Shared {
    running: bool,
    outcome: Option<Outcome>,
}

struct Outcome {
    data: Arc<Data>,
    /// The key of what a count or a take-back said, if one was asked for.
    note: Option<&'static str>,
}

/// The state of the page.
pub struct FinanceState {
    /// Where the files are; none on the sample page.
    host: Option<Host>,
    data: Option<Arc<Data>>,
    shared: Arc<Mutex<Shared>>,
    period: Period,
    from: String,
    to: String,
    balance: String,
    /// Wrong input the field names: the balance.
    balance_error: bool,
    /// A count is on its way: the buttons wait.
    counting: bool,
    /// How many lines of the statement show.
    shown: usize,
    last_frame: Option<u64>,
    refreshed: Option<f64>,
    /// Said by the last count, taken by the window.
    notice: Option<String>,
}

impl FinanceState {
    /// The state of the sample page: made-up numbers, nothing is read or written.
    pub fn sample() -> FinanceState {
        FinanceState {
            host: None,
            data: Some(Arc::new(Data::sample())),
            shared: Arc::default(),
            period: Period::Week,
            from: String::new(),
            to: String::new(),
            balance: String::new(),
            balance_error: false,
            counting: false,
            shown: STATEMENT_PAGE,
            last_frame: None,
            refreshed: None,
            notice: None,
        }
    }

    /// The state for the data folder of `paths`: the first look reads the saved files in the background.
    pub fn of(paths: &Paths) -> FinanceState {
        let period = paths.setting(PERIOD_SETTING).and_then(|word| Period::from_word(&word)).unwrap_or(Period::Week);
        FinanceState { host: Some(paths.host.clone()), data: None, period, ..FinanceState::sample() }
    }

    pub fn is_sample(&self) -> bool {
        self.host.is_none()
    }

    /// The window takes what the page said (to show in the corner).
    pub fn take_notice(&mut self) -> Option<String> {
        self.notice.take()
    }

    fn running(&self) -> bool {
        self.shared.lock().unwrap_or_else(|e| e.into_inner()).running
    }

    /// Takes the data of a finished run, if there is one.
    fn poll(&mut self) {
        let taken = self.shared.lock().unwrap_or_else(|e| e.into_inner()).outcome.take();
        let Some(outcome) = taken else {
            if self.counting && !self.running() {
                self.counting = false;
            }
            return;
        };
        if let Some(note) = outcome.note {
            self.counting = false;
            if note == "s_fi_gespeichert" {
                self.balance.clear();
            }
            self.notice = Some(t(note));
        }
        self.data = Some(outcome.data);
    }

    /// Starts a run in the background; false when one is on already.
    fn start(&mut self, job: Job, ctx: &egui::Context, now: f64) -> bool {
        let Some(host) = self.host.clone() else {
            // The sample page reads and writes nothing: it only says so.
            match job {
                Job::Count(_) => {
                    self.notice = Some(t("s_fi_gespeichert"));
                    self.balance.clear();
                }
                Job::Undo => self.notice = Some(t("s_fi_zurueckgenommen")),
                Job::Refresh => {}
            }
            return true;
        };
        {
            let mut shared = self.shared.lock().unwrap_or_else(|e| e.into_inner());
            if shared.running {
                return false;
            }
            shared.running = true;
        }
        if !matches!(job, Job::Refresh) {
            self.counting = true;
        }
        self.refreshed = Some(now);
        let (shared, ctx, lock) = (Arc::clone(&self.shared), ctx.clone(), writeguard::state());
        let first = self.data.is_none();
        versekit_core::language::spawn_with_language(move || {
            writeguard::adopt(&lock);
            run(&host, job, first, &shared, &ctx);
        });
        true
    }

    /// Waits for the run on, for tests.
    #[cfg(test)]
    pub(crate) fn wait(&mut self) {
        for _ in 0..400 {
            self.poll();
            if !self.running() {
                self.poll();
                return;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        panic!("the run does not end");
    }
}

/// The run of a thread: the saved files first (for a page opened the first time), then the running log, then
/// the count if one was asked for, then the data once more.
fn run(host: &Host, job: Job, first: bool, shared: &Mutex<Shared>, ctx: &egui::Context) {
    // Whatever happens in the thread: the next run may start.
    struct Release<'a>(&'a Mutex<Shared>);
    impl Drop for Release<'_> {
        fn drop(&mut self) {
            self.0.lock().unwrap_or_else(|e| e.into_inner()).running = false;
        }
    }
    let _release = Release(shared);
    let paths = Paths::new(host.clone());
    let publish = |data: Data, note: Option<&'static str>| {
        shared.lock().unwrap_or_else(|e| e.into_inner()).outcome = Some(Outcome { data: Arc::new(data), note });
        ctx.request_repaint();
    };
    if first {
        publish(Data::load(&paths), None);
    }
    if stock_journal::enabled(&paths) {
        let running: Vec<String> = paths
            .game_folder()
            .map(|game| paths.host.flavor.join(&game, "Game.log"))
            .filter(|log| std::path::Path::new(log).is_file())
            .into_iter()
            .collect();
        if !running.is_empty() {
            let source = LogSource::new(&paths, LogSource::sources_of(&paths));
            stock_journal::catch_up(&source, &running);
        }
    }
    let note = match job {
        Job::Count(balance) => {
            let data = Data::load(&paths);
            let mut counts = data.counts.clone();
            let made = finance::count(counts.last(), balance, now_seconds(), &data.sessions);
            counts.push(made);
            Some(if finance::save_counts(&paths, &counts) { "s_fi_gespeichert" } else { "s_fi_nicht_gespeichert" })
        }
        Job::Undo => {
            let kept = finance::withdraw_last(&finance::counts(&paths));
            Some(if finance::save_counts(&paths, &kept) { "s_fi_zurueckgenommen" } else { "s_fi_nicht_gespeichert" })
        }
        Job::Refresh => None,
    };
    publish(Data::load(&paths), note);
}

// ---------------------------------------------------------------------------
// Drawing
// ---------------------------------------------------------------------------

/// Money as shown: hidden in streamer mode.
fn money(value: f64) -> String {
    if streamer::active() {
        return streamer::placeholder();
    }
    formats::money(language::current(), value)
}

fn signed(value: f64) -> String {
    if streamer::active() {
        return streamer::placeholder();
    }
    signed_money(language::current(), value)
}

/// The text of a sum that is only an estimate.
fn about(text: String) -> String {
    tf("s_lj_ca", &[("0", Arg::from(text))])
}

fn period_name(period: Period) -> String {
    t(match period {
        Period::Today => "s_fi_zeitraum_heute",
        Period::Session => "s_fi_zeitraum_sitzung",
        Period::Week => "s_fi_zeitraum_woche",
        Period::Month => "s_fi_zeitraum_monat",
        Period::Custom => "s_fi_zeitraum_frei",
    })
}

fn before_name(before: Before) -> String {
    t(match before {
        Before::Yesterday => "s_fi_vor_gestern",
        Before::PreviousSession => "s_fi_vor_sitzung",
        Before::PreviousWeek => "s_fi_vor_woche",
        Before::PreviousMonth => "s_fi_vor_monat",
        Before::Same => "s_fi_vor_gleich",
    })
}

/// What the card of the page says on the overview: the balance of the account (or what is missing for one), and
/// under it the result of today and when the balance was entered. Read from the files; the names of the goods
/// are not needed for the figures.
pub(crate) fn glance(paths: &Paths, lang: language::Lang) -> (String, Vec<String>) {
    // The rewards of the contracts take seconds to make: they are made beside the window, the card says it reads.
    static MAKING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if !contract_rewards::kept_is_current(paths) {
        if !MAKING.swap(true, std::sync::atomic::Ordering::SeqCst) {
            let paths = Paths::new(paths.host.clone());
            std::thread::spawn(move || {
                let _ = contract_rewards::Rewards::load_kept(&paths);
                MAKING.store(false, std::sync::atomic::Ordering::SeqCst);
            });
        }
        return (language::t_in(lang, "s_uv_lese"), Vec::new());
    }
    let counts = finance::counts(paths);
    let sessions = if stock_journal::enabled(paths) { stock_journal::sessions(&stock_journal::load(paths)) } else { Vec::new() };
    let nobody = |_: &str, _: &str| None;
    let nowhere = |_: &str| None;
    let contracts = contracts_of(paths);
    let account = finance::account_with(&counts, &sessions, &contracts, &finance::Namer { item: &nobody, place: &nowhere });
    let (Some(balance), Some(last)) = (account.balance(), counts.last()) else {
        return (language::t_in(lang, "s_fi_konto_leer"), Vec::new());
    };
    let shown = |value: f64, plus: bool| {
        if streamer::active() {
            streamer::placeholder()
        } else if plus {
            signed_money(lang, value)
        } else {
            formats::money(lang, value)
        }
    };
    let now = now_seconds();
    let ask = Ask { period: Period::Today, now, sessions: &[], custom: None };
    let today = finance::windows(&ask, &finance::local_offset)
        .map_or(0.0, |(window, ..)| finance::totals(&sessions, &contracts, window).result());
    let one = |key: &str, text: String| language::tf_in(lang, key, &[("0", Arg::from(text))]);
    (
        one("s_fi_karte_konto", shown(balance, false)),
        vec![one("s_fi_karte_heute", shown(today, true)), one("s_fi_karte_stand", stamp(lang, last.time))],
    )
}

/// What the window and the one before it hold.
struct View {
    window: Window,
    now: Totals,
    before: Totals,
    before_kind: Before,
    play: i64,
}

/// The free period of the two fields: the first and the last day as the starts of the local days; `None` while
/// a field is empty or cannot be read.
fn custom_days(from: &str, to: &str) -> Option<(i64, i64)> {
    let day = |text: &str| {
        let (year, month, day) = finance::parse_date(text)?;
        finance::day_of_date(year, month, day, &finance::local_offset)
    };
    Some((day(from)?, day(to)?))
}

fn view(data: &Data, period: Period, custom: Option<(i64, i64)>) -> Option<View> {
    let starts: Vec<i64> = data.spans.iter().map(|(from, _)| *from).collect();
    let ask = Ask { period, now: data.now, sessions: &starts, custom };
    let (window, before, before_kind) = finance::windows(&ask, &finance::local_offset)?;
    Some(View {
        now: finance::totals(&data.sessions, &data.contracts, window),
        before: finance::totals(&data.sessions, &data.contracts, before),
        play: finance::play_seconds(&data.spans, window),
        window,
        before_kind,
    })
}

/// The balance as typed: whole aUEC, separators allowed; `None` for anything else.
fn parse_balance(text: &str) -> Option<i64> {
    let digits: String = text.chars().filter(|c| !matches!(c, '.' | ',' | ' ' | '\'' | '\u{a0}')).collect();
    (!digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit())).then(|| digits.parse().ok()).flatten()
}

/// Draws the page. The window shows what the page said in the corner through [`FinanceState::take_notice`].
pub fn show_page(ui: &mut Ui, scheme: &Scheme, state: &mut FinanceState) {
    state.poll();
    let now = ui.input(|i| i.time);
    let frame = ui.ctx().cumulative_frame_nr();
    let entered = state.last_frame.is_none_or(|last| frame > last + 1);
    state.last_frame = Some(frame);
    if !state.is_sample() && (entered || state.refreshed.is_none_or(|at| now - at >= REFRESH_SECONDS)) {
        state.start(Job::Refresh, ui.ctx(), now);
    }
    if state.running() {
        ui.ctx().request_repaint_after(Duration::from_millis(250));
    } else if !state.is_sample() {
        ui.ctx().request_repaint_after(Duration::from_secs_f64(REFRESH_SECONDS));
    }
    small(ui, scheme, &t("s_fi_lead"));
    ui.add_space(GAP);
    let Some(data) = state.data.clone() else {
        widgets::loading(ui, scheme, &t("s_fi_laedt"));
        return;
    };
    if data.journal_off {
        let text = tf("s_fi_aus", &[("0", Arg::from(t("s_lj_schalter"))), ("1", Arg::from(t("hf_meinlager")))]);
        widgets::status_box(ui, scheme, scheme.gold, "info", &t("hf_finanzen"), &text);
        ui.add_space(GAP);
    }
    account_section(ui, scheme, state, &data);
    section(ui, scheme, &t("s_fi_auswertung"), "");
    periods(ui, scheme, state);
    if state.period == Period::Custom {
        custom_fields(ui, scheme, state);
    }
    let chosen = custom_days(&state.from, &state.to);
    match view(&data, state.period, chosen) {
        Some(view) => figures(ui, scheme, &data, &view),
        None => {
            ui.add_space(GAP);
            let key = if state.period == Period::Session { "s_fi_keine_sitzung" } else { "s_fi_frei_leer" };
            widgets::empty_state(ui, scheme, "clock", &t(key));
        }
    }
    notes(ui, scheme, "s_fi_hinweis");
}

fn periods(ui: &mut Ui, scheme: &Scheme, state: &mut FinanceState) {
    let names: Vec<String> = Period::ALL.iter().map(|p| period_name(*p)).collect();
    let names: Vec<&str> = names.iter().map(String::as_str).collect();
    let mut chosen = Period::ALL.iter().position(|p| *p == state.period).unwrap_or(0);
    if widgets::tab_bar(ui, scheme, &names, &mut chosen) {
        state.period = Period::ALL[chosen.min(Period::ALL.len() - 1)];
        if let Some(host) = &state.host {
            Paths::new(host.clone()).set_setting(PERIOD_SETTING, Value::str(state.period.word()));
        }
    }
    ui.add_space(6.0);
}

/// The two fields of the free period; a date that cannot be read, or a first day after the last, says so under
/// its field.
fn custom_fields(ui: &mut Ui, scheme: &Scheme, state: &mut FinanceState) {
    // Each field has a hint of its own: the id of a field comes from its hint.
    let (from_hint, to_hint) = (t("s_fi_von_hint"), t("s_fi_bis_hint"));
    let day = |text: &str| {
        finance::parse_date(text).and_then(|(y, m, d)| finance::day_of_date(y, m, d, &finance::local_offset))
    };
    let unreadable = |text: &str| (!text.trim().is_empty() && day(text).is_none()).then(|| t("s_fi_datum_falsch"));
    let from_error = unreadable(&state.from);
    let mut to_error = unreadable(&state.to);
    if from_error.is_none()
        && to_error.is_none()
        && let (Some(first), Some(last)) = (day(&state.from), day(&state.to))
        && first > last
    {
        to_error = Some(t("s_fi_datum_reihenfolge"));
    }
    ui.horizontal_wrapped(|row| {
        let label = row.label(t("s_fi_von"));
        let field = widgets::checked_field(row, scheme, &mut state.from, &from_hint, 170.0, from_error.as_deref());
        field.labelled_by(label.id);
        let label = row.label(t("s_fi_bis"));
        let field = widgets::checked_field(row, scheme, &mut state.to, &to_hint, 170.0, to_error.as_deref());
        field.labelled_by(label.id);
    });
    ui.add_space(6.0);
}

/// The tiles, the sources and the lines that name what is only an estimate.
fn figures(ui: &mut Ui, scheme: &Scheme, data: &Data, view: &View) {
    let lang = language::current();
    let (now, before) = (&view.now, &view.before);
    let last = view.window.to - 1;
    small(
        ui,
        scheme,
        &tf(
            "s_fi_spanne",
            &[("0", Arg::from(moment(lang, view.window.from))), ("1", Arg::from(moment(lang, last.min(data.now))))],
        ),
    );
    ui.add_space(GAP);
    if now.is_empty() && before.is_empty() {
        widgets::empty_state(ui, scheme, "hand-coins", &t("s_fi_leer"));
        return;
    }
    let name = before_name(view.before_kind);
    let versus = |value: f64, then: f64| {
        tf(
            "s_fi_vorher",
            &[("0", Arg::from(name.clone())), ("1", Arg::from(money(then))), ("2", Arg::from(signed(value - then)))],
        )
    };
    let result = now.result();
    let result_text = signed(result);
    let per_hour = finance::per_hour(now.income(), view.play);
    let per_hour_before = per_hour
        .map_or_else(|| t("s_fi_zu_kurz"), |_| tf("s_fi_spielzeit", &[("0", Arg::from(playtime::as_text(view.play)))]));
    let cards = [
        tile(t("s_fi_einnahmen"), money(now.income()), versus(now.income(), before.income())),
        tile(t("s_fi_ausgaben"), money(now.spent()), versus(now.spent(), before.spent())),
        tile(t("s_fi_ergebnis"), result_text, versus(result, before.result())),
        tile(t("s_fi_pro_stunde"), per_hour.map_or_else(|| NONE.to_string(), money), per_hour_before),
    ];
    let wide = ui.available_width() >= 4.0 * TILE_MIN + 3.0 * GAP;
    tiles(ui, scheme, &cards, if wide { 4 } else { 2 });
    small(ui, scheme, &t("s_fi_stunde_h"));
    if now.income_requested() > 0.0 {
        ui.add_space(4.0);
        small(ui, scheme, &tf("s_fi_davon_angefragt", &[("0", Arg::from(money(now.income_requested())))]));
    }
    section(ui, scheme, &t("s_fi_quellen"), "");
    sources(ui, now);
    ui.add_space(4.0);
    small(ui, scheme, &contracts_line(now));
}

/// The table by source: income, spending and how many deals.
fn sources(ui: &mut Ui, now: &Totals) {
    let count = |n: usize| crate::statistics_page::count(n as i64);
    let dash = || (NONE.to_string(), None);
    let plain = |text: String| (text, None);
    let trade_in = money(now.trade.sum);
    let trade_count = if now.trade.requested_count > 0 {
        tf(
            "s_lj_n_angefragt",
            &[("0", Arg::from(count(now.trade.count))), ("1", Arg::from(count(now.trade.requested_count)))],
        )
    } else {
        count(now.trade.count)
    };
    let shop_count = now.shop_sales.count + now.shop_purchases.count;
    let contracts_in = match (now.contracts_priced, now.contracts_approx) {
        (0, _) => NONE.to_string(),
        (_, 0) => money(now.contracts_reward),
        _ => about(money(now.contracts_reward)),
    };
    // What it cost to take them (the buy-in): spending, in the column of its own.
    let contracts_out = match (now.contracts_cost_count, now.contracts_cost_approx > 0.0) {
        (0, _) => NONE.to_string(),
        (_, false) => money(now.contracts_cost),
        _ => about(money(now.contracts_cost)),
    };
    let rows: Vec<Vec<Cell>> = vec![
        vec![plain(t("s_fi_q_handel")), plain(trade_in), dash(), plain(trade_count)],
        vec![
            plain(t("s_fi_q_laeden")),
            plain(money(now.shop_sales.sum)),
            plain(money(now.shop_purchases.sum)),
            plain(count(shop_count)),
        ],
        vec![
            plain(t("s_fi_q_ueberweisung")),
            plain(money(now.transfers_in.sum)),
            plain(money(now.transfers_out.sum)),
            plain(count(now.transfers_in.count + now.transfers_out.count)),
        ],
        vec![plain(t("s_fi_q_auftraege")), plain(contracts_in), plain(contracts_out), plain(count(now.contracts))],
    ];
    let headers = [t("s_fi_q_quelle"), t("s_fi_q_ein"), t("s_fi_q_aus"), t("s_fi_q_anzahl")];
    table(ui, "finanzen-quellen", &headers, &rows, &[false, true, true, true]);
}

/// What the log says of the contracts of the window, and what taking some of them cost.
fn contracts_line(now: &Totals) -> String {
    let rewards = rewards_line(now);
    if now.contracts_cost_count == 0 {
        return rewards;
    }
    let count = crate::statistics_page::count(now.contracts_cost_count as i64);
    let money_text = if now.contracts_cost_approx > 0.0 { about(money(now.contracts_cost)) } else { money(now.contracts_cost) };
    let cost = tf("s_fi_auftraege_einsatz", &[("0", Arg::from(count)), ("1", Arg::from(money_text))]);
    format!("{rewards} {cost}")
}

/// What the amounts of the completed contracts of the window say.
fn rewards_line(now: &Totals) -> String {
    let count = |n: usize| crate::statistics_page::count(n as i64);
    let approx = now.contracts_approx > 0;
    let main = if now.contracts == 0 {
        return t("s_fi_auftraege_keine");
    } else if now.contracts == 1 {
        match (now.contracts_priced, approx) {
            (0, _) => return t("s_fi_auftrag_ohne"),
            (_, true) => return tf("s_fi_auftrag_mit_ca", &[("0", Arg::from(money(now.contracts_reward)))]),
            _ => return tf("s_fi_auftrag_mit", &[("0", Arg::from(money(now.contracts_reward)))]),
        }
    } else if now.contracts_priced == 0 {
        return tf("s_fi_auftraege_ohne", &[("0", Arg::from(count(now.contracts)))]);
    } else {
        tf(
            "s_fi_auftraege_teils",
            &[
                ("0", Arg::from(count(now.contracts))),
                ("1", Arg::from(count(now.contracts_priced))),
                ("2", Arg::from(money(now.contracts_reward))),
            ],
        )
    };
    if !approx {
        return main;
    }
    let low = tf(
        "s_fi_auftraege_ca",
        &[("0", Arg::from(count(now.contracts_approx))), ("1", Arg::from(money(now.income_approx())))],
    );
    format!("{main} {low}")
}

/// The account: the balance in large figures, the field for the balance of the game, the statement.
fn account_section(ui: &mut Ui, scheme: &Scheme, state: &mut FinanceState, data: &Data) {
    let lang = language::current();
    section(ui, scheme, &t("s_fi_konto"), "");
    match (data.account.balance(), data.counts.last()) {
        (Some(balance), Some(last)) => {
            let size = egui::TextStyle::Heading.resolve(ui.style()).size;
            ui.add(
                egui::Label::new(RichText::new(money(balance)).font(crate::fonts::bold(size)).color(crate::color(scheme.accent)))
                    .wrap(),
            );
            small(
                ui,
                scheme,
                &tf(
                    "s_fi_konto_stand",
                    &[("0", Arg::from(moment(lang, last.time))), ("1", Arg::from(money(last.balance as f64)))],
                ),
            );
        }
        _ => {
            ui.add(egui::Label::new(RichText::new(t("s_fi_konto_leer")).strong()).wrap());
            small(ui, scheme, &t("s_fi_konto_leer_h"));
        }
    }
    ui.add_space(GAP);
    let error = state.balance_error.then(|| t("s_fi_kasse_falsch"));
    ui.horizontal_wrapped(|row| {
        let label = row.label(t("s_fi_kasse_feld"));
        let field =
            widgets::checked_field(row, scheme, &mut state.balance, &t("s_fi_kasse_hint"), 160.0, error.as_deref());
        field.labelled_by(label.id);
        let busy = state.counting;
        let reason = busy.then(|| t("s_sa_gesperrt"));
        let text = t("s_fi_kasse_knopf");
        let button = widgets::Button::new(&text, ButtonKind::Strong, scheme).blocked(reason.as_deref());
        if row.add(button).clicked() {
            match parse_balance(&state.balance) {
                Some(balance) => {
                    state.balance_error = false;
                    let now = row.input(|i| i.time);
                    state.start(Job::Count(balance), row.ctx(), now);
                }
                None => state.balance_error = true,
            }
        }
        if !data.counts.is_empty() {
            let text = t("s_fi_zurueck_knopf");
            let button = widgets::Button::new(&text, ButtonKind::Normal, scheme).blocked(reason.as_deref());
            if row.add(button).on_hover_text(t("s_fi_zurueck_h")).clicked() {
                let now = row.input(|i| i.time);
                state.start(Job::Undo, row.ctx(), now);
            }
        }
        if busy {
            widgets::loading(row, scheme, &t("s_fi_arbeitet"));
        }
    });
    small(ui, scheme, &t("s_fi_kasse_h"));
    if data.account.entries.is_empty() {
        return;
    }
    ui.add_space(GAP);
    section(ui, scheme, &t("s_fi_auszug"), "");
    statement(ui, scheme, state, &data.account);
}

/// The day and the time of a moment, local: `21.11.2026 14:05`.
fn stamp(lang: language::Lang, seconds: i64) -> String {
    let Some(time) = formats::local_time(seconds) else { return String::new() };
    let day = match lang {
        language::Lang::De => formats::strftime("%d.%m.%Y", &time),
        language::Lang::En => formats::strftime("%Y-%m-%d", &time),
    };
    format!("{day} {}", formats::strftime_shown("%H:%M", &time))
}

/// What a line of the statement says it is.
fn booking_text(entry: &Entry) -> String {
    let named = |fallback: &str| entry.what.clone().unwrap_or_else(|| t(fallback));
    let with_count = |name: String| {
        if entry.quantity > 1 { tf("s_fi_b_stueck", &[("0", Arg::from(entry.quantity)), ("1", Arg::from(name))]) } else { name }
    };
    match entry.kind {
        Booking::Opening => t("s_fi_b_start"),
        Booking::Check => t("s_fi_b_abgleich"),
        Booking::TransferIn => t("s_fi_b_ue_ein"),
        Booking::TransferOut => t("s_fi_b_ue_aus"),
        Booking::ContractDone => match &entry.what {
            Some(name) => tf("s_fi_b_auftrag", &[("0", Arg::from(name.clone()))]),
            None => t("s_fi_b_auftrag_ohne_name"),
        },
        Booking::ContractCost => match &entry.what {
            Some(name) => tf("s_fi_b_auftrag_einsatz", &[("0", Arg::from(name.clone()))]),
            None => t("s_fi_b_auftrag_einsatz_ohne_name"),
        },
        Booking::ItemSold => tf("s_fi_b_verkauf", &[("0", Arg::from(with_count(named("s_fi_b_gegenstand"))))]),
        Booking::ItemBought => tf("s_fi_b_kauf", &[("0", Arg::from(with_count(named("s_fi_b_gegenstand"))))]),
        Booking::GoodsSold => {
            let (name, scu) = (named("s_fi_b_ware"), Arg::from(entry.quantity));
            match &entry.place {
                Some(place) => tf(
                    "s_fi_b_ware_ort",
                    &[("0", Arg::from(name)), ("1", scu), ("2", Arg::from(place.clone()))],
                ),
                None => tf("s_fi_b_ware_scu", &[("0", Arg::from(name)), ("1", scu)]),
            }
        }
    }
}

/// The statement: newest line first; per line what it is and when, the booking with its sign and the balance after.
fn statement(ui: &mut Ui, scheme: &Scheme, state: &mut FinanceState, account: &Account) {
    let lang = language::current();
    let plain = |text: String| (text, None);
    let rows: Vec<Vec<Cell>> = account
        .entries
        .iter()
        .rev()
        .take(state.shown)
        .map(|entry| {
            let when = stamp(lang, entry.time);
            let when = if entry.requested { tf("s_fi_b_anfrage", &[("0", Arg::from(when))]) } else { when };
            let amount = match entry.kind {
                Booking::Opening => NONE.to_string(),
                _ if entry.approx => about(signed(entry.amount)),
                _ => signed(entry.amount),
            };
            vec![plain(format!("{}\n{when}", booking_text(entry))), plain(amount), plain(money(entry.balance))]
        })
        .collect();
    let headers = [t("s_fi_k_buchung"), t("s_fi_k_betrag"), t("s_fi_k_stand")];
    table(ui, "finanzen-auszug", &headers, &rows, &[false, true, true]);
    let left = account.entries.len().saturating_sub(state.shown);
    if left > 0 {
        ui.add_space(4.0);
        let word = tf("s_af_mehr", &[("n", Arg::Int(left.min(STATEMENT_PAGE) as i64))]);
        if ui.add(widgets::Button::new(&word, ButtonKind::Normal, scheme)).clicked() {
            state.shown += STATEMENT_PAGE;
        }
    }
}

#[cfg(test)]
mod tests;
