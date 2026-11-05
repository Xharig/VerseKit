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

//! The five pages of the group "Statistik" (`stats_pages.py`): overview (with
//! the evaluation of the logs, which 4.12 had on a page of its own), ships &
//! equipment, contracts, quantum travel, stability.
//!
//! ⚠ **Show, then read after.** A page shows the saved state of
//! `statistik.json` at once; the running `Game.log` is read in the
//! background when the page is opened (and now and then while it stays open).
//! The evaluation runs always, there is no switch for it in 5.0.
//!
//! ⚠ **Nothing is clickable** except the buttons of the evaluation on the
//! overview. The pages tell, they lead nowhere: bars and tables are no buttons.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use egui::{RichText, TextStyle, Ui};
use versekit_core::formats;
use versekit_core::language::{self, Arg, Lang, t, tf};
use versekit_core::logsource::LogSource;
use versekit_core::mission_log;
use versekit_core::paths::{Host, Paths};
use versekit_core::stats_report::{
    self as report, End, Evaluation, Missions, Names, Quantum, ReportNames, Session, Ships, Stability, Summary,
    WeaponSlot,
};
use versekit_core::stock_journal::{
    self,
    spending::{Spending, spending},
};
use versekit_core::theme::{Color, Scheme};
use versekit_core::{play_stats, playtime, shard, writeguard};

use crate::layout::{self, GAP};
use crate::widgets::{self, ButtonKind, RankTone};
use crate::{color, file_dialog};

/// Narrowest a tile gets before the tiles stack.
const TILE_MIN: f32 = 170.0;
/// How often the open page reads the running log again (seconds).
const REFRESH_SECONDS: f64 = 300.0;
/// What stands for a value that is not there (no date, no file).
const NONE: &str = "-";
/// Every how many hours the heat map has a number.
const HOUR_STEP: usize = 3;

/// The five pages, in the order of the menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    Overview,
    Ships,
    Missions,
    Quantum,
    Stability,
}

impl Page {
    /// The page of a menu key (`hf_st_…`); the key of the former evaluation page leads to the overview.
    pub fn of(key: &str) -> Option<Page> {
        Some(match key {
            "hf_st_auswertung" | "hf_st_uebersicht" => Page::Overview,
            "hf_st_schiffe" => Page::Ships,
            "hf_st_auftraege" => Page::Missions,
            "hf_st_quantum" => Page::Quantum,
            "hf_st_stabil" => Page::Stability,
            _ => return None,
        })
    }

    fn lead(self) -> &'static str {
        match self {
            Page::Overview => "s_sx_lead",
            Page::Ships => "s_ss_lead",
            Page::Missions => "s_sm_lead",
            Page::Quantum => "s_sq_lead",
            Page::Stability => "s_sb_lead",
        }
    }
}

// ---------------------------------------------------------------------------
// What the pages show
// ---------------------------------------------------------------------------

/// Everything the five pages show, worked out of the saved files.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Report {
    pub evaluation: Evaluation,
    pub summary: Summary,
    pub ships: Ships,
    pub missions: Missions,
    pub quantum: Quantum,
    pub stability: Stability,
    pub names: ReportNames,
    /// What the purchases of the log add up to; empty while the reading of the stock lines is off.
    pub spending: Spending,
    /// For the export: the sessions, the periods and the session running now.
    sessions: Vec<Session>,
    spans: Vec<(i64, i64)>,
    running: Option<(i64, i64)>,
    own: Option<String>,
}

impl Report {
    /// Reads the saved files of the data folder and counts. `own` is the own
    /// account (a session of another one does not count), `last_run` the
    /// moment of the last look in this program.
    pub fn build(paths: &Paths, own: Option<&str>, last_run: Option<f64>) -> Report {
        let sessions = report::load_sessions(paths);
        let spans = playtime::spans(paths);
        let running = playtime::running_span(paths);
        let log = mission_log::load(paths);
        let summary = report::summary(&sessions, &spans, running, own);
        let ships = report::ships(&sessions, own);
        let quantum = report::quantum(&sessions, own);
        let stability = report::stability(&sessions, own);
        for join in stability.shards.iter().flat_map(|(_, joins)| joins) {
            shard::keep_private(&join.shard);
        }
        Report {
            spending: spending_of(paths),
            evaluation: report::evaluation(paths, last_run),
            names: report::report_names(paths, &ships, &summary, &quantum),
            missions: report::missions(&sessions, &log, own),
            stability,
            summary,
            ships,
            quantum,
            sessions,
            spans,
            running,
            own: own.map(str::to_string),
        }
    }

    /// Made-up numbers for the sample page: nothing of the user's.
    pub fn sample() -> Report {
        let spending = Spending {
            total: 1_820_400.0,
            purchases: 37,
            today: 12_500.0,
            week: 148_000.0,
            last_session: Some((1_792_300_000, 31_200.0)),
        };
        let weekday = |hours: [i64; 24]| hours.map(|h| h * 600);
        let mut heat = [[0; 24]; 7];
        heat[1] = weekday([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2, 4, 5, 5, 3, 1, 0]);
        heat[3] = weekday([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 3, 5, 6, 6, 4, 2, 0]);
        heat[5] = weekday([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 2, 3, 3, 4, 4, 4, 5, 5, 3, 0, 0, 0, 0]);
        let pairs = |list: &[(&str, i64)]| list.iter().map(|(name, n)| (name.to_string(), *n)).collect::<Vec<_>>();
        let weapons = |slot: &'static str, list: &[(&str, i64)]| {
            let top = pairs(list);
            WeaponSlot { slot, total: top.iter().map(|(_, n)| n).sum(), top }
        };
        let last = |start: i64, duration: i64, end: End, drops: i64| report::Last { start, duration, end, drops };
        // The contracts are those of the sample mission log, so both pages tell the same.
        let log = crate::contract_log::LogData::sample();
        let from_log = report::missions(&[], &log.entries, None);
        Report {
            evaluation: Evaluation { last: Some(1_792_400_000.0), sessions: 41, files: 38, size: 52_300 },
            summary: Summary {
                playtime: 41 * 3600 + 20 * 60,
                sessions: 41,
                average: 3630,
                longest: 5 * 3600 + 12 * 60,
                contracts: 128,
                failed: 9,
                jumps: 460,
                deaths: 17,
                last_death: Some(1_792_300_000),
                lost: 6,
                most_lost: Some("AEGS_Gladius".to_string()),
                recorded: 40,
                since: Some(1_781_000_000),
                heat,
            },
            ships: Ships {
                ships: 7,
                most: Some(("AEGS_Gladius".to_string(), 22)),
                sessions_with_ship: 38,
                used: pairs(&[("AEGS_Gladius", 22), ("DRAK_Cutlass_Black", 11), ("MISC_Freelancer", 6)]),
                lost: pairs(&[("AEGS_Gladius", 4), ("DRAK_Cutlass_Black", 2)]),
                lost_total: 6,
                weapons: vec![
                    weapons("ruecken", &[("behr_rifle_ballistic_01", 31), ("klwe_rifle_energy_01", 12)]),
                    weapons("seite", &[("behr_pistol_ballistic_01", 40)]),
                    weapons("hand", &[]),
                ],
            },
            missions: Missions {
                completed: 128,
                failed: 9,
                abandoned: 14,
                frequent: from_log.frequent,
                latest: from_log.latest,
            },
            quantum: Quantum {
                jumps: 460,
                selections: 590,
                targets: pairs(&[
                    ("MISSION_QT", 80),
                    ("NavPoint_Dynamic", 45),
                    ("rs_ext_pyro6_leo", 30),
                    ("levski_all-001", 22),
                ]),
                starts: pairs(&[("Stanton", 70), ("Pyro", 38)]),
            },
            stability: Stability {
                sessions: 40,
                clean: 31,
                crashed: 3,
                open: 6,
                reasons: vec![("Timeout".to_string(), 5), (report::REASONS[0].0.to_string(), 3)],
                drops: 8,
                latest: vec![
                    last(1_792_300_000, 5400, End::Clean, 0),
                    last(1_792_200_000, 3000, End::Crash, 1),
                    last(1_792_100_000, 7000, End::Open, 2),
                ],
                shards: Some((
                    1_792_300_000,
                    vec![
                        shard::Join {
                            at: Some(1_792_300_060),
                            shard: shard::Shard { id: "pub_euw1b_10000001_100".to_string() },
                        },
                        shard::Join {
                            at: Some(1_792_302_400),
                            shard: shard::Shard { id: "pub_euw1b_10000002_200".to_string() },
                        },
                    ],
                )),
            },
            spending,
            ..Report::default()
        }
    }
}

/// The spending of the journal: today by the local day; nothing while the reading is off or the journal is empty.
fn spending_of(paths: &Paths) -> Spending {
    if !stock_journal::enabled(paths) {
        return Spending::default();
    }
    let sessions = stock_journal::sessions(&stock_journal::load(paths));
    let day =
        |seconds: i64| formats::local_time(seconds).map(|t| formats::strftime("%Y-%m-%d", &t)).unwrap_or_default();
    spending(&sessions, now_seconds() as i64, &day)
}

// ---------------------------------------------------------------------------
// Work in the background
// ---------------------------------------------------------------------------

/// What a run reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Job {
    /// The running `Game.log` only: what a page does when it is opened.
    Refresh,
    /// "Jetzt auswerten": every log, only the new or grown ones are read.
    Evaluate,
    /// "Alles neu auswerten": every log there is, read once more.
    Rescan,
}

/// What a run hands over.
struct Outcome {
    report: Arc<Report>,
    own: Option<String>,
    /// New sessions found; `None` while only the saved state is shown.
    added: Option<usize>,
    /// The moment the run was done (seconds since 1970).
    ran: Option<f64>,
    finished: bool,
}

#[derive(Default)]
struct Shared {
    running: bool,
    outcome: Option<Outcome>,
}

fn now_seconds() -> f64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0.0, |d| d.as_secs_f64())
}

/// The state of the five pages.
pub struct StatsState {
    /// Where the files are; none on the sample page.
    host: Option<Host>,
    report: Option<Arc<Report>>,
    shared: Arc<Mutex<Shared>>,
    own: Option<String>,
    /// When the account was asked for in vain (window time): it is asked
    /// again after five minutes, not with every look.
    own_tried: Option<f64>,
    last_run: Option<f64>,
    /// The frame and the page of the last drawing: a gap or another page means
    /// a page was opened.
    last_frame: Option<u64>,
    last_page: Option<Page>,
    refreshed: Option<f64>,
    /// A run the user started is on: the buttons wait, a line says so.
    working: Option<Job>,
    dialog: Option<file_dialog::Pending>,
    /// Said by the last run or export, taken by the window.
    notice: Option<String>,
}

impl StatsState {
    /// The state of the sample page: made-up numbers, nothing is read.
    pub fn sample() -> StatsState {
        StatsState {
            host: None,
            report: Some(Arc::new(Report::sample())),
            shared: Arc::default(),
            own: None,
            own_tried: None,
            last_run: None,
            last_frame: None,
            last_page: None,
            refreshed: None,
            working: None,
            dialog: None,
            notice: None,
        }
    }

    /// The state for the data folder of `paths`: the first look reads the
    /// saved state in the background.
    pub fn of(paths: &Paths) -> StatsState {
        StatsState { host: Some(paths.host.clone()), report: None, ..StatsState::sample() }
    }

    /// Is this made-up?
    pub fn is_sample(&self) -> bool {
        self.host.is_none()
    }

    /// Takes the report of a finished run, if there is one.
    fn poll(&mut self) {
        let taken = self.shared.lock().unwrap_or_else(|e| e.into_inner()).outcome.take();
        let Some(outcome) = taken else {
            // A run that died without a word must not leave the buttons locked.
            if self.working.is_some() && !self.running() {
                self.working = None;
            }
            return;
        };
        if outcome.own.is_some() {
            self.own = outcome.own;
        }
        if let Some(ran) = outcome.ran {
            self.last_run = Some(ran);
        }
        if outcome.finished {
            let job = self.working.take();
            if let (Some(job), Some(added)) = (job, outcome.added)
                && job != Job::Refresh
            {
                let key = if added == 1 { "s_sa_fertig_1" } else { "s_sa_fertig" };
                self.notice = Some(tf(key, &[("0", Arg::from(count(added as i64)))]));
            }
        }
        self.report = Some(outcome.report);
    }

    fn running(&self) -> bool {
        self.shared.lock().unwrap_or_else(|e| e.into_inner()).running
    }

    /// Starts a run in the background; false when one is on already.
    fn start(&mut self, job: Job, ctx: &egui::Context, now: f64) -> bool {
        let Some(host) = self.host.clone() else {
            // The sample page reads nothing: the run is over at once.
            if job != Job::Refresh {
                self.notice = Some(tf("s_sa_fertig", &[("0", Arg::from(3))]));
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
        if job != Job::Refresh {
            self.working = Some(job);
        }
        self.refreshed = Some(now);
        let ask_own = self.own.is_none() && self.own_tried.is_none_or(|at| now - at > 300.0);
        if ask_own {
            self.own_tried = Some(now);
        }
        let (shared, ctx, lock) = (Arc::clone(&self.shared), ctx.clone(), writeguard::state());
        let request = Request {
            host,
            job,
            known_own: self.own.clone(),
            ask_own,
            last_run: self.last_run,
            first: self.report.is_none(),
        };
        std::thread::spawn(move || {
            writeguard::adopt(&lock);
            run(&request, &shared, &ctx);
        });
        true
    }

    /// Hands the report to the export: asks where to save it.
    fn ask_export(&mut self, ctx: &egui::Context) {
        let name = format!("versekit-statistik-{}.json", today_text());
        // The sample page writes nothing and opens no dialog: it only says so.
        let Some(host) = &self.host else {
            self.notice = Some(tf("s_sx_exportiert", &[("0", Arg::from(name))]));
            return;
        };
        let paths = Paths::new(host.clone());
        let start = versekit_core::export::FileExports {
            paths: &paths,
            version: env!("CARGO_PKG_VERSION"),
            clock: versekit_core::export::now_unix,
        }
        .folder();
        let start = Some(start);
        let filters: file_dialog::Filters = &[("JSON", &["json"])];
        self.dialog = Some(file_dialog::save_file(ctx, &t("s_sx_export"), filters, start.as_deref(), &name));
    }

    /// The answer of the save dialog: writes the file.
    fn finish_export(&mut self) {
        let Some(answer) = self.dialog.as_ref().and_then(file_dialog::Pending::take) else { return };
        self.dialog = None;
        // Cancelled: nothing is said.
        let Some(mut target) = answer.filter(|p| !p.is_empty()) else { return };
        if !target.to_lowercase().ends_with(".json") {
            target.push_str(".json");
        }
        self.write_export(&target);
    }

    /// Writes the statistics to `target` and says whether it worked.
    fn write_export(&mut self, target: &str) {
        let Some(report) = &self.report else { return };
        let value = report::export_value(
            &report.sessions,
            &report.spans,
            report.running,
            report.own.as_deref(),
            env!("CARGO_PKG_VERSION"),
            versekit_core::export::now_unix(),
        );
        self.notice = Some(if report::export(target, &value) {
            let name =
                std::path::Path::new(target).file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned());
            tf("s_sx_exportiert", &[("0", Arg::from(name))])
        } else {
            t("s_sx_export_fehler")
        });
    }

    /// The window takes what the pages said (to show in the corner).
    pub fn take_notice(&mut self) -> Option<String> {
        self.notice.take()
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

/// What a run needs to know.
struct Request {
    host: Host,
    job: Job,
    /// The own account if it is known; else it is asked for when `ask_own`.
    known_own: Option<String>,
    ask_own: bool,
    last_run: Option<f64>,
    /// No report yet: the saved state is handed over before the logs are read.
    first: bool,
}

/// The run of a thread: first the saved state (for a page opened the first
/// time), then the logs, then the report once more.
fn run(request: &Request, shared: &Mutex<Shared>, ctx: &egui::Context) {
    let Request { host, job, known_own, ask_own, last_run, first } = request;
    let (job, ask_own, last_run, first) = (*job, *ask_own, *last_run, *first);
    let known_own = known_own.clone();
    // Whatever happens in the thread: the next run may start.
    struct Release<'a>(&'a Mutex<Shared>);
    impl Drop for Release<'_> {
        fn drop(&mut self) {
            self.0.lock().unwrap_or_else(|e| e.into_inner()).running = false;
        }
    }
    let _release = Release(shared);
    let paths = Paths::new(host.clone());
    let source = LogSource::new(&paths, LogSource::sources_of(&paths));
    // The account of the settings counts first, read anew with every run: who
    // changes it sees the new figures without a restart.
    let configured = paths.setting(versekit_core::logsource::ACCOUNT_SETTING).filter(|a| !a.trim().is_empty());
    let mut own = configured.or(known_own);
    let publish = |report: Report, own: &Option<String>, added: Option<usize>, ran: Option<f64>, finished: bool| {
        let mut shared = shared.lock().unwrap_or_else(|e| e.into_inner());
        shared.outcome = Some(Outcome { report: Arc::new(report), own: own.clone(), added, ran, finished });
        drop(shared);
        ctx.request_repaint();
    };
    // Show first, then look further: the search for the account may touch every backup.
    if first {
        publish(Report::build(&paths, own.as_deref(), last_run), &own, None, None, false);
    }
    if own.is_none() && ask_own {
        own = source.own_account(None);
    }
    fetch_names(&paths);
    let files = match job {
        Job::Refresh => paths
            .game_folder()
            .map(|game| paths.host.flavor.join(&game, "Game.log"))
            .filter(|running| std::path::Path::new(running).is_file())
            .into_iter()
            .collect(),
        Job::Evaluate | Job::Rescan => play_stats::log_files(&paths),
    };
    let clock = now_seconds;
    let added = if job == Job::Rescan {
        play_stats::rescan(&source, &files, &clock)
    } else {
        play_stats::catch_up(&source, &files, &clock)
    };
    let ran = now_seconds();
    publish(Report::build(&paths, own.as_deref(), Some(ran)), &own, Some(added), Some(ran), true);
}

/// How often a run looked for the names, for the tests.
#[cfg(test)]
static NAME_LOOKS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// Whether this run of the program asked for the names of ships and items already.
static NAMES_ASKED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// The names the pages show, before the logs are read (`stats_pages.live_page` with `names=True`): the names
/// of ships and items once per run of the program (read from the archive where the saved ones are missing or
/// belong to another state of it), and the place names where they are not saved yet.
fn fetch_names(paths: &Paths) {
    #[cfg(test)]
    NAME_LOOKS.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    // The tests of this crate never read the archive of a game.
    if cfg!(test) {
        return;
    }
    if !NAMES_ASKED.swap(true, std::sync::atomic::Ordering::SeqCst) {
        let _ = versekit_gamedata::game_text::names_or_fetch(paths, None);
    }
    let _ = versekit_gamedata::game_text::places_or_fetch(paths, None);
}

fn today_text() -> String {
    formats::local_time(now_seconds() as i64).map_or_else(String::new, |t| formats::strftime("%Y-%m-%d", &t))
}

// ---------------------------------------------------------------------------
// Drawing blocks
// ---------------------------------------------------------------------------

fn count(n: i64) -> String {
    formats::count(language::current(), n)
}

fn small(ui: &mut Ui, scheme: &Scheme, text: &str) {
    ui.add(egui::Label::new(RichText::new(text).small().color(color(scheme.sub))).wrap());
}

/// One figure of a tile: its title, the number, a line below.
struct Tile {
    title: String,
    value: String,
    below: String,
}

fn tile(title: impl Into<String>, value: impl Into<String>, below: impl Into<String>) -> Tile {
    Tile { title: title.into(), value: value.into(), below: below.into() }
}

/// Figures as tiles of equal width and, in a row, equal height, `columns`
/// side by side; under each other when the page is too narrow.
fn tiles(ui: &mut Ui, scheme: &Scheme, id: &str, items: &[Tile], columns: usize) {
    for (row, chunk) in items.chunks(columns).enumerate() {
        if row > 0 {
            ui.add_space(GAP);
        }
        let key = ui.make_persistent_id((id, "tile-row", row));
        let side_by_side = ui.available_width() >= columns as f32 * TILE_MIN + (columns - 1) as f32 * GAP;
        let tallest: f32 = if side_by_side { ui.ctx().data(|d| d.get_temp(key)).unwrap_or(0.0) } else { 0.0 };
        let mut measured: f32 = 0.0;
        layout::columns_or_stack(ui, TILE_MIN, columns, |ui, i| {
            let Some(item) = chunk.get(i) else { return };
            let shown = widgets::card(ui, scheme, |ui| {
                ui.set_min_width(ui.available_width());
                ui.set_min_height(tallest);
                let top = ui.cursor().min.y;
                small(ui, scheme, &item.title);
                let size = TextStyle::Heading.resolve(ui.style()).size;
                ui.add(
                    egui::Label::new(
                        RichText::new(&item.value).font(crate::fonts::bold(size)).color(color(scheme.accent)),
                    )
                    .wrap(),
                );
                if !item.below.is_empty() {
                    small(ui, scheme, &item.below);
                }
                measured = measured.max(ui.cursor().min.y - top);
            });
            let _ = shown;
        });
        if side_by_side && (measured - tallest).abs() > 0.5 {
            ui.ctx().data_mut(|d| d.insert_temp(key, measured));
            ui.ctx().request_repaint();
        }
    }
}

/// The head of a section: a line, the title, at the right a sum.
fn section(ui: &mut Ui, scheme: &Scheme, title: &str, right: &str) {
    ui.add_space(14.0);
    let (line, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 1.0), egui::Sense::hover());
    ui.painter().rect_filled(line, 0.0, color(scheme.line));
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.add(egui::Label::new(RichText::new(title).strong()).wrap());
        if !right.is_empty() {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(RichText::new(right).small().color(color(scheme.sub)));
            });
        }
    });
    ui.add_space(4.0);
}

/// A ranking: the bar behind each line, measured at the largest entry, not
/// at the sum, or ten similar entries would all have tiny bars.
fn ranking(ui: &mut Ui, scheme: &Scheme, rows: &[(String, i64)], tone: RankTone, unit: &dyn Fn(i64) -> String) {
    if rows.is_empty() {
        small(ui, scheme, &t("s_sx_nichts"));
        return;
    }
    let largest = rows.iter().map(|(_, n)| *n).max().unwrap_or(0).max(1);
    for (name, n) in rows {
        let share = (*n as f32 / largest as f32).max(0.01);
        widgets::ranking_row_tone(ui, scheme, name, &unit(*n), share, tone);
        ui.add_space(2.0);
    }
}

fn times(n: i64) -> String {
    tf("s_sx_mal", &[("0", Arg::from(count(n)))])
}

/// One cell of a table: text and its color (the usual if none).
type Cell = (String, Option<Color>);

/// A plain table: bold head, stripes, no click. The first column takes the
/// room and is cut with "…" where it is too long.
///
/// `numbers` marks the columns of numbers: they stand at the right.
fn table(ui: &mut Ui, id: &str, headers: &[String], rows: &[Vec<Cell>], numbers: &[bool]) {
    let at_right = |column: usize| numbers.get(column).copied().unwrap_or(false);
    let place = |column: usize, ui: &mut Ui, label: egui::Label| {
        // Only the first column is cut: the others are as wide as their widest cell, and a label that may be cut
        // reports no width of its own to size them by.
        let label = if column == 0 { label.truncate() } else { label };
        if at_right(column) {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| ui.add(label));
        } else {
            ui.add(label);
        }
    };
    let mut builder = egui_extras::TableBuilder::new(ui)
        .id_salt(id)
        .striped(true)
        .vscroll(false)
        .cell_layout(egui::Layout::left_to_right(egui::Align::Center));
    for index in 0..headers.len() {
        builder = if index == 0 {
            builder.column(egui_extras::Column::remainder().at_least(90.0).clip(true))
        } else {
            builder.column(egui_extras::Column::auto().at_least(60.0))
        };
    }
    builder
        .header(24.0, |mut header| {
            for (column, text) in headers.iter().enumerate() {
                header.col(|ui| place(column, ui, egui::Label::new(RichText::new(text).strong())));
            }
        })
        .body(|mut body| {
            for cells in rows {
                body.row(22.0, |mut row| {
                    for (column, (text, tone)) in cells.iter().enumerate() {
                        row.col(|ui| {
                            let rich = match tone {
                                Some(tone) => RichText::new(text).color(color(*tone)),
                                None => RichText::new(text),
                            };
                            place(column, ui, egui::Label::new(rich));
                        });
                    }
                });
            }
        });
}

/// "Zu den Zahlen": what the figures mean and what they do not know.
///
/// Folded up: the explanations are long, and who knows the figures does not
/// want to read them with every look.
fn notes(ui: &mut Ui, scheme: &Scheme, key: &str) {
    ui.add_space(GAP);
    widgets::collapsing(&t("s_sx_zahlen")).default_open(false).show(ui, |ui| small(ui, scheme, &t(key)));
    ui.add_space(GAP);
}

/// A page with nothing recorded says so once, with the way to get something.
fn nothing_yet(ui: &mut Ui, scheme: &Scheme, icon: &str) {
    widgets::empty_state(ui, scheme, icon, &t("s_sx_nichts"));
}

fn day(lang: Lang, seconds: i64) -> String {
    report::local_day(lang, seconds)
}

fn moment(lang: Lang, seconds: i64) -> String {
    report::local_moment(lang, seconds)
}

// ---------------------------------------------------------------------------
// The pages
// ---------------------------------------------------------------------------

/// Draws a page. Returns what it wants said in the corner of the window.
pub fn show(ui: &mut Ui, scheme: &Scheme, state: &mut StatsState, page: Page) -> Option<String> {
    state.poll();
    state.finish_export();
    let now = ui.input(|i| i.time);
    // Entering the page reads the running log; staying on it, once a minute.
    let frame = ui.ctx().cumulative_frame_nr();
    let entered = state.last_frame.is_none_or(|last| frame > last + 1) || state.last_page != Some(page);
    state.last_frame = Some(frame);
    state.last_page = Some(page);
    if !state.is_sample() && (entered || state.refreshed.is_none_or(|at| now - at >= REFRESH_SECONDS)) {
        state.start(Job::Refresh, ui.ctx(), now);
    }
    if state.running() {
        ui.ctx().request_repaint_after(Duration::from_millis(250));
    } else if !state.is_sample() {
        ui.ctx().request_repaint_after(Duration::from_secs_f64(REFRESH_SECONDS));
    }
    small(ui, scheme, &t(page.lead()));
    ui.add_space(GAP);
    let lang = language::current();
    let Some(report) = state.report.clone() else {
        // (an `Arc`: the clone is a count, not the data)
        widgets::loading(ui, scheme, &t("s_sa_laedt"));
        return state.take_notice();
    };
    match page {
        Page::Overview => overview(ui, scheme, state, &report, lang),
        Page::Ships => ships(ui, scheme, &report),
        Page::Missions => missions(ui, scheme, &report, lang),
        Page::Quantum => quantum(ui, scheme, &report),
        Page::Stability => stability(ui, scheme, &report, lang),
    }
    state.take_notice()
}

/// The buttons that read the logs with what they do, and the rows "Automatisch auswerten" and "Statistik speichern".
fn evaluation(ui: &mut Ui, scheme: &Scheme, state: &mut StatsState) {
    // Whether a run of the page itself (opening, every few minutes) or of a button is on:
    // the buttons wait for it, and the line beside them says why.
    let busy = state.working.is_some() || (state.running() && !state.is_sample());
    let (wait, why) = (t("s_sa_laeuft"), t("s_sa_gesperrt"));
    let mut pressed = None;
    let (evaluate_text, rescan_text) = (t("s_sa_jetzt"), t("s_sa_neu"));
    ui.horizontal_wrapped(|row| {
        let reason = busy.then_some(why.as_str());
        let evaluate = widgets::Button::new(&evaluate_text, ButtonKind::Strong, scheme).blocked(reason);
        if row.add(evaluate).clicked() {
            pressed = Some(Job::Evaluate);
        }
        let rescan = widgets::Button::new(&rescan_text, ButtonKind::Normal, scheme).blocked(reason);
        if row.add(rescan).clicked() {
            pressed = Some(Job::Rescan);
        }
        if busy {
            widgets::loading(row, scheme, &wait);
        }
    });
    if let Some(job) = pressed {
        let now = ui.input(|i| i.time);
        state.start(job, ui.ctx(), now);
    }
    ui.add_space(4.0);
    small(ui, scheme, &t("s_sa_knoepfe_h"));
    ui.add_space(GAP);
    // The evaluation always runs: a row that tells, without a switch.
    crate::settings::setting_row(ui, scheme, &t("s_sa_auto"), &t("s_sa_auto_h"), false, |_| {});
    let export_name = t("s_sx_export");
    let mut export = false;
    crate::settings::setting_row(
        ui,
        scheme,
        export_name.trim_end_matches([' ', '…']),
        &t("s_sa_export_h"),
        false,
        |row| {
            export = row.add(widgets::Button::new(&export_name, ButtonKind::Normal, scheme)).clicked();
        },
    );
    if export {
        state.ask_export(ui.ctx());
    }
}

/// The overview: the state of the evaluation and the figures of the play time as cards of one width, the
/// heat map, then the buttons that read the logs. With no session recorded yet the figures give way to a line
/// that says so; the state of the evaluation and the buttons stay.
fn overview(ui: &mut Ui, scheme: &Scheme, state: &mut StatsState, report: &Report, lang: Lang) {
    let data = &report.summary;
    let info = &report.evaluation;
    let size = if info.size > 0 {
        format!("{} KB", count((info.size as f64 / 1024.0).round_ties_even() as i64))
    } else {
        NONE.to_string()
    };
    let last = info.last.map_or_else(|| t("s_sa_nie"), |at| moment(lang, at as i64));
    let mut cards = vec![
        tile(t("s_sa_zuletzt"), last, ""),
        tile(t("s_sa_sitzungen"), count(info.sessions as i64), ""),
        tile(t("s_sa_dateien"), count(info.files as i64), ""),
        tile(t("s_sa_groesse"), size, ""),
    ];
    let recorded = data.playtime > 0;
    if recorded {
        let names = &report.names.things;
        let since = data.since.map_or_else(String::new, |at| tf("s_sx_seit", &[("0", Arg::from(day(lang, at)))]));
        let per_session =
            if data.recorded > 0 { (data.jumps as f64 / data.recorded as f64).round_ties_even() as i64 } else { 0 };
        let last_death =
            data.last_death.map_or_else(String::new, |at| tf("s_sx_letzter_tod", &[("0", Arg::from(day(lang, at)))]));
        let most_lost = data.most_lost.as_deref().map_or_else(String::new, |class| {
            tf("s_sx_meist_verloren", &[("0", Arg::from(report::display_name(class, true, names)))])
        });
        cards.extend([
            tile(t("s_sx_spielzeit"), playtime::as_text(data.playtime), since),
            tile(
                t("s_sx_sitzungen"),
                count(data.sessions),
                tf(
                    "s_sx_schnitt",
                    &[
                        ("0", Arg::from(playtime::as_text(data.average))),
                        ("1", Arg::from(playtime::as_text(data.longest))),
                    ],
                ),
            ),
            tile(t("s_sx_auftraege"), count(data.contracts), tf("s_sx_fehl", &[("0", Arg::from(count(data.failed)))])),
            tile(t("s_sx_spruenge"), count(data.jumps), tf("s_sx_je", &[("0", Arg::from(count(per_session)))])),
            tile(t("s_sx_tode"), count(data.deaths), last_death),
            tile(t("s_sx_verloren"), count(data.lost), most_lost),
        ]);
    }
    // Ten cards stand five and five or two and two; never a row that is half empty.
    let wide = cards.len() > 4 && ui.available_width() >= 5.0 * TILE_MIN + 4.0 * GAP;
    tiles(ui, scheme, "uebersicht", &cards, if wide { 5 } else { 2 });
    if recorded {
        let title = t("s_sx_wann");
        section(ui, scheme, &title, "");
        let days = t("s_sx_tage");
        let labels: Vec<&str> = days.split(',').collect();
        let rows: Vec<Vec<f64>> = data.heat.iter().map(|row| row.iter().map(|n| *n as f64).collect()).collect();
        let cell = |row: usize, hour: usize, seconds: f64| {
            format!(
                "{} {hour:02}:00, {}",
                labels.get(row).copied().unwrap_or_default(),
                playtime::as_text(seconds as i64)
            )
        };
        widgets::heat_map_tipped(
            ui,
            scheme,
            &labels,
            &rows,
            HOUR_STEP,
            Some(&widgets::HeatTips { title: &title, cell: &cell }),
        );
    } else {
        ui.add_space(GAP);
        widgets::empty_state(ui, scheme, "chart-no-axes-combined", &t("s_sx_leer"));
    }
    shop_spending(ui, scheme, &report.spending, lang);
    section(ui, scheme, &t("hf_st_auswertung"), "");
    evaluation(ui, scheme, state);
    notes(ui, scheme, "s_sx_hinweis");
}

/// "Ausgegeben im Laden": what the purchases of the log add up to, today, in a week, in the last session
/// and in all. Nothing is drawn without a purchase or while the reading of the stock lines is off.
fn shop_spending(ui: &mut Ui, scheme: &Scheme, spending: &Spending, lang: Lang) {
    if spending.is_empty() {
        return;
    }
    section(ui, scheme, &t("s_lj_ausgaben"), "");
    small(ui, scheme, &t("s_lj_ausgaben_h"));
    ui.add_space(GAP);
    let (session, session_below) = match spending.last_session {
        Some((start, sum)) => (formats::money(lang, sum), tf("s_lj_sitzung_am", &[("0", Arg::from(day(lang, start)))])),
        None => (NONE.to_string(), String::new()),
    };
    let cards = [
        tile(t("s_lj_heute"), formats::money(lang, spending.today), ""),
        tile(t("s_lj_woche"), formats::money(lang, spending.week), ""),
        tile(t("s_lj_sitzung"), session, session_below),
        tile(
            t("s_lj_gesamt"),
            formats::money(lang, spending.total),
            tf("s_lj_kaeufe_n", &[("0", Arg::from(count(spending.purchases as i64)))]),
        ),
    ];
    let wide = ui.available_width() >= 4.0 * TILE_MIN + 3.0 * GAP;
    tiles(ui, scheme, "laden", &cards, if wide { 4 } else { 2 });
}

fn ships(ui: &mut Ui, scheme: &Scheme, report: &Report) {
    let data = &report.ships;
    if data.used.is_empty() && data.lost.is_empty() && data.weapons.iter().all(|w| w.top.is_empty()) {
        return nothing_yet(ui, scheme, "plane-takeoff");
    }
    let names = &report.names.things;
    let vehicle = |class: &str| report::display_name(class, true, names);
    if let Some((class, sessions)) = &data.most {
        tiles(
            ui,
            scheme,
            "schiffe",
            &[
                tile(t("s_ss_meist"), vehicle(class), ""),
                tile(t("s_ss_sitzungen_mit"), count(*sessions), ""),
                tile(t("s_ss_schiffe"), count(data.ships as i64), ""),
                tile(t("s_ss_anteil"), format!("{} %", count(report::percent(*sessions, data.sessions_with_ship))), ""),
            ],
            2,
        );
    }
    let named = |list: &[(String, i64)]| -> Vec<(String, i64)> { list.iter().map(|(c, n)| (vehicle(c), *n)).collect() };
    section(ui, scheme, &t("s_ss_genutzt"), "");
    ranking(ui, scheme, &named(&data.used), RankTone::Accent, &times);
    section(ui, scheme, &t("s_ss_verloren"), &count(data.lost_total));
    ranking(ui, scheme, &named(&data.lost), RankTone::Red, &times);
    for (slot, key) in [("ruecken", "s_ss_ruecken"), ("seite", "s_ss_seite"), ("hand", "s_ss_hand")] {
        let found = data.weapons.iter().find(|w| w.slot == slot);
        let (top, total) = found.map_or((&[][..], 0), |w| (w.top.as_slice(), w.total));
        section(ui, scheme, &t(key), "");
        let rows: Vec<(String, i64)> =
            top.iter().map(|(class, n)| (report::display_name(class, false, names), *n)).collect();
        ranking(ui, scheme, &rows, RankTone::Gold, &|n| format!("{} %", count(report::percent(n, total))));
    }
    notes(ui, scheme, "s_ss_hinweis");
}

fn missions(ui: &mut Ui, scheme: &Scheme, report: &Report, lang: Lang) {
    let data = &report.missions;
    let ended = data.completed + data.failed + data.abandoned;
    if ended == 0 && data.frequent.is_empty() && data.latest.is_empty() {
        return nothing_yet(ui, scheme, "crosshair");
    }
    let quote = |n: i64| tf("s_sm_quote", &[("0", Arg::from(count(report::percent(n, ended))))]);
    tiles(
        ui,
        scheme,
        "auftraege",
        &[
            tile(t("s_sm_fertig"), count(data.completed), quote(data.completed)),
            tile(t("s_sm_fehl"), count(data.failed), quote(data.failed)),
            tile(t("s_sm_abbruch"), count(data.abandoned), quote(data.abandoned)),
        ],
        3,
    );
    section(ui, scheme, &t("s_sm_haeufig"), "");
    ranking(ui, scheme, &data.frequent, RankTone::Accent, &times);
    section(ui, scheme, &t("s_sm_letzte"), "");
    if data.latest.is_empty() {
        small(ui, scheme, &t("s_sx_nichts"));
    } else {
        let rows: Vec<Vec<Cell>> = data
            .latest
            .iter()
            .map(|entry| {
                let (key, tone) = match entry.state.as_str() {
                    mission_log::ABORTED => ("s_al_abbruch", scheme.red_pale),
                    mission_log::FAILED => ("s_al_fehl", scheme.red_pale),
                    mission_log::EXPIRED => ("s_al_verfallen", scheme.sub),
                    _ => ("s_al_fertig", scheme.accent),
                };
                let when = report::log_seconds(&entry.stamp).map_or_else(|| NONE.to_string(), |s| moment(lang, s));
                vec![(entry.name.clone(), None), (when, Some(scheme.sub)), (t(key), Some(tone))]
            })
            .collect();
        table(ui, "auftraege-letzte", &[t("s_sm_name"), t("s_sx_datum"), t("s_sm_ausgang")], &rows, &[false; 3]);
    }
    notes(ui, scheme, "s_sm_hinweis");
}

fn place_label(key: &str, names: &Names) -> String {
    let place = report::place_parts(key, names);
    match place.text_key {
        None => place.name,
        Some(text_key) => {
            let text = t(text_key);
            if text.contains("{0}") { tf(text_key, &[("0", Arg::from(place.name))]) } else { text }
        }
    }
}

fn quantum(ui: &mut Ui, scheme: &Scheme, report: &Report) {
    let data = &report.quantum;
    if data.jumps == 0 && data.selections == 0 && data.targets.is_empty() && data.starts.is_empty() {
        return nothing_yet(ui, scheme, "orbit");
    }
    tiles(
        ui,
        scheme,
        "quantum",
        &[
            tile(t("s_sq_angekommen"), count(data.jumps), t("s_sq_angekommen_u")),
            tile(t("s_sq_wahl"), count(data.selections), t("s_sq_wahl_u")),
            tile(
                t("s_sq_quote"),
                format!("{} %", count(report::percent(data.jumps, data.selections))),
                t("s_sq_quote_u"),
            ),
        ],
        3,
    );
    section(ui, scheme, &t("s_sq_ziele"), "");
    let targets: Vec<(String, i64)> =
        data.targets.iter().map(|(key, n)| (place_label(key, &report.names.places), *n)).collect();
    ranking(ui, scheme, &targets, RankTone::Accent, &times);
    // The starting points stand in the log in the game's language: no translation.
    section(ui, scheme, &t("s_sq_starts"), "");
    ranking(ui, scheme, &data.starts, RankTone::Gold, &times);
    notes(ui, scheme, "s_sq_hinweis");
}

fn stability(ui: &mut Ui, scheme: &Scheme, report: &Report, lang: Lang) {
    let data = &report.stability;
    if data.sessions == 0 {
        return nothing_yet(ui, scheme, "activity");
    }
    let share = |n: i64| tf("s_sx_anteil", &[("0", Arg::from(count(report::percent(n, data.sessions))))]);
    tiles(
        ui,
        scheme,
        "stabil",
        &[
            tile(t("s_sb_sauber"), count(data.clean), share(data.clean)),
            tile(t("s_sb_absturz"), count(data.crashed), share(data.crashed)),
            tile(t("s_sb_offen"), count(data.open), share(data.open)),
        ],
        3,
    );
    shards(ui, scheme, report, lang);
    section(ui, scheme, &t("s_sb_gruende"), &count(data.drops));
    let reasons: Vec<(String, i64)> = data
        .reasons
        .iter()
        .map(|(reason, n)| (report::reason_key(reason).map_or_else(|| reason.clone(), t), *n))
        .collect();
    ranking(ui, scheme, &reasons, RankTone::Accent, &count);
    section(ui, scheme, &t("s_sb_letzte"), "");
    if data.latest.is_empty() {
        small(ui, scheme, &t("s_sx_nichts"));
    } else {
        let rows: Vec<Vec<Cell>> = data
            .latest
            .iter()
            .map(|last| {
                let (key, tone) = match last.end {
                    End::Clean => ("s_sb_e_sauber", scheme.accent),
                    // Pale red: the strong one is too dark on every second line of the table.
                    End::Crash => ("s_sb_e_absturz", scheme.red_pale),
                    End::Open => ("s_sb_e_offen", scheme.gold),
                };
                let start = if last.start != 0 { moment(lang, last.start) } else { NONE.to_string() };
                vec![
                    (start, None),
                    (playtime::as_text(last.duration.max(0)), Some(scheme.sub)),
                    (t(key), Some(tone)),
                    (count(last.drops), Some(scheme.sub)),
                ]
            })
            .collect();
        table(
            ui,
            "stabil-letzte",
            &[t("s_sx_datum"), t("s_sb_dauer"), t("s_sb_ende"), t("s_sb_abbrueche")],
            &rows,
            &[false, true, false, true],
        );
    }
    notes(ui, scheme, "s_sb_hinweis");
}

/// How far the start of the running play time may lie from the start of a session that is the same one (seconds).
const SAME_START: i64 = 60;

/// The shard of the newest session: each join with its time, region and number, and how often the shard changed.
/// The session running now is called so; otherwise it is the last one.
fn shards(ui: &mut Ui, scheme: &Scheme, report: &Report, lang: Lang) {
    let Some((from, joins)) = &report.stability.shards else { return };
    let running = report.running.is_some_and(|(start, _)| (start - from).abs() <= SAME_START);
    let title = t(if running { "s_sb_shard" } else { "s_sb_shard_letzte" });
    if joins.is_empty() {
        section(ui, scheme, &title, "");
        small(ui, scheme, &t("s_sb_shard_keiner"));
        return;
    }
    let changes = tf("s_sb_shard_wechsel", &[("0", Arg::from(count(shard::changes(joins) as i64)))]);
    section(ui, scheme, &title, &changes);
    let rows: Vec<Vec<Cell>> = joins
        .iter()
        .map(|join| {
            vec![
                (join.at.map_or_else(|| NONE.to_string(), |at| moment(lang, at)), None),
                (join.shard.region_label(lang), Some(scheme.sub)),
                (join.shard.number(), None),
            ]
        })
        .collect();
    table(
        ui,
        "stabil-shard",
        &[t("s_sb_shard_zeit"), t("s_sb_shard_region"), t("s_sb_shard_nr")],
        &rows,
        &[false, false, false],
    );
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use egui::Pos2;
    use versekit_core::jsonio::Value;
    use versekit_core::language::{Choice, set_thread_language};
    use versekit_core::legacy::ENV_HOME;
    use versekit_core::paths::ENV_INSTALL_DIR;
    use versekit_core::pypath::Flavor;
    use versekit_core::theme;

    use super::*;

    const START: i64 = 1_790_000_000;

    fn world(tag: &str) -> (PathBuf, Paths) {
        let root = std::env::temp_dir().join(format!("versekit-statistik-{tag}-{}", versekit_core::testdir::run_tag()));
        let _ = std::fs::remove_dir_all(&root);
        let (home, game) = (root.join("home"), root.join("LIVE"));
        std::fs::create_dir_all(&home).unwrap();
        std::fs::create_dir_all(&game).unwrap();
        // A game folder of its own, or the real installation of the computer applies.
        let old = std::time::SystemTime::now() - Duration::from_secs(3 * 86_400);
        std::fs::write(game.join("Game.log"), "<2026-10-01T10:00:00.000Z> Log started\n").unwrap();
        std::fs::File::options().write(true).open(game.join("Game.log")).unwrap().set_modified(old).unwrap();
        let (home_text, game_text) = (home.to_string_lossy().into_owned(), game.to_string_lossy().into_owned());
        let host = Host::new(
            Flavor::native(),
            &[(ENV_HOME, home_text.as_str()), (ENV_INSTALL_DIR, game_text.as_str())],
            root.to_str().unwrap(),
        );
        (root, Paths::new(host))
    }

    fn write(paths: &Paths, file: &str, text: &str) {
        std::fs::write(paths.app_file(file), text).unwrap();
    }

    /// Two own sessions and one of a stranger: every figure of the pages has a known source.
    fn fill(paths: &Paths) {
        write(paths, "einstellungen.json", r#"{"eigener_account": "Xharig"}"#);
        let a = START;
        let b = START + 100_000;
        let c = START + 200_000;
        let d = START + 300_000;
        write(
            paths,
            "statistik.json",
            &format!(
                r#"{{"format": 4, "gelesen": {{"a.log": 10, "b.log": 20, "c.log": 30, "d.log": 40}}, "stand": {b}.5, "sitzungen": {{
"{a}": {{"von": {a}, "bis": {a_end}, "sauber": true, "account": "Xharig", "auftraege": 4, "fehlgeschlagen": 1, "abgebrochen": 2,
  "spruenge": 10, "schiffe": ["AEGS_Sabre"], "verloren": ["AEGS_Sabre"], "tode": [{a}], "waffen": {{"ruecken": {{"behr_rifle": 2}}}},
  "zielwahlen": 12, "ziele": {{"rs_ext_pyro6_leo": 5, "NavPoint_Dynamic": 2}}, "starts": {{"Stanton": 3}}, "absturz": false,
  "abbrueche": {{"Timeout": 2}}}},
"{b}": {{"von": {b}, "bis": {b_end}, "sauber": true, "account": "Xharig", "auftraege": 3, "fehlgeschlagen": 0, "abgebrochen": 0,
  "spruenge": 6, "schiffe": ["AEGS_Sabre", "DRAK_Cutlass_Black"], "verloren": [], "tode": [], "waffen": {{}},
  "zielwahlen": 8, "ziele": {{"rs_ext_pyro6_leo": 1}}, "starts": {{}}, "absturz": true, "abbrueche": {{"Nub destroyed": 1}}}},
"{c}": {{"von": {c}, "bis": {c_end}, "sauber": true, "account": "Fremder", "auftraege": 50, "fehlgeschlagen": 40, "abgebrochen": 0,
  "spruenge": 99, "schiffe": ["ORIG_300i"], "verloren": [], "tode": [], "waffen": {{}}, "zielwahlen": 99, "ziele": {{}}, "starts": {{}},
  "absturz": false, "abbrueche": {{}}}},
"{d}": {{"von": {d}, "bis": {d_end}, "sauber": true, "account": "Xharig", "auftraege": 0, "fehlgeschlagen": 0, "abgebrochen": 0,
  "spruenge": 0, "schiffe": ["MISC_Freelancer"], "verloren": ["DRAK_Cutlass_Black"], "tode": [{d}, {d_second}], "waffen": {{}},
  "zielwahlen": 0, "ziele": {{}}, "starts": {{}}, "absturz": false, "abbrueche": {{}}}}
}}}}"#,
                a_end = a + 7200,
                b_end = b + 3600,
                c_end = c + 1800,
                d_end = d + 1800,
                d_second = d + 100,
            ),
        );
        write(
            paths,
            "spielzeit.json",
            &format!(
                r#"{{"format": 1, "sitzungen": [{{"von": {a}, "bis": {}}}, {{"von": {b}, "bis": {}}}, {{"von": {c}, "bis": {}}}, {{"von": {d}, "bis": {}}}], "gelesen": {{}}}}"#,
                a + 7200,
                b + 3600,
                c + 1800,
                d + 1800
            ),
        );
        write(
            paths,
            "auftragslog.json",
            r#"{"format": 2, "auftraege": [
{"name": "Retake the Outpost", "wann": "2026-10-01T10:00:00", "bis": "2026-10-01T11:00:00", "zustand": "abgeschlossen"},
{"name": "Retake the Outpost", "wann": "2026-10-02T10:00:00", "bis": "2026-10-02T11:00:00", "zustand": "fehlgeschlagen"},
{"name": "Verified Bounty: A Very Long Name Of A Wanted Person With A Ship Class And A Support Note Behind It And More", "wann": "2026-10-02T12:00:00", "bis": "2026-10-02T13:00:00", "zustand": "abgeschlossen"},
{"name": "Deliver Supplies", "wann": "2026-10-03T10:00:00", "zustand": "laeuft"}]}"#,
        );
    }

    /// The texts of a drawing with the corner they begin at, in drawing order.
    fn placed(shapes: &[egui::epaint::ClippedShape]) -> Vec<(String, Pos2)> {
        fn collect(shape: &egui::Shape, into: &mut Vec<(String, Pos2)>) {
            match shape {
                egui::Shape::Text(text) => into.push((text.galley.text().to_string(), text.pos)),
                egui::Shape::Vec(shapes) => shapes.iter().for_each(|shape| collect(shape, into)),
                _ => {}
            }
        }
        let mut into = Vec::new();
        shapes.iter().for_each(|clipped| collect(&clipped.shape, &mut into));
        into
    }

    /// Draws a page for a few frames at this width; returns its texts and how far it sticks out.
    fn draw(state: &mut StatsState, page: Page, width: f32) -> (Vec<String>, f32) {
        draw_in(state, page, width, "original")
    }

    /// [`draw`] in the colors of a scheme.
    fn draw_in(state: &mut StatsState, page: Page, width: f32, scheme_id: &str) -> (Vec<String>, f32) {
        let (shown, overflow) = draw_placed(state, page, width, scheme_id);
        (shown.into_iter().map(|(text, _)| text).collect(), overflow)
    }

    /// [`draw_in`] with the place of every text.
    fn draw_placed(state: &mut StatsState, page: Page, width: f32, scheme_id: &str) -> (Vec<(String, Pos2)>, f32) {
        thread_local! {
            static CTX: egui::Context = {
                let ctx = egui::Context::default();
                crate::install(&ctx);
                ctx
            };
        }
        let scheme = theme::by_id(scheme_id).unwrap();
        let (mut shown, mut overflow) = (Vec::new(), 0.0);
        CTX.with(|ctx| {
            for _ in 0..3 {
                let input = egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, egui::vec2(width, 2000.0))),
                    ..Default::default()
                };
                let mut output = ctx.run_ui(input, |ui| {
                    show(ui, scheme, state, page);
                    overflow = (ui.min_rect().width() - ui.max_rect().width()).max(0.0);
                });
                shown = placed(&output.shapes);
                output.textures_delta.clear();
            }
        });
        (shown, overflow)
    }

    /// A state on real files, read to the end.
    fn real(tag: &str) -> (PathBuf, Paths, StatsState) {
        let (root, paths) = world(tag);
        fill(&paths);
        let mut state = StatsState::of(&paths);
        // The first drawing starts the reading; wait for it, then draw what it found.
        draw(&mut state, Page::Overview, 900.0);
        state.wait();
        (root, paths, state)
    }

    fn has(texts: &[String], part: &str) -> bool {
        texts.iter().any(|text| text.contains(part))
    }

    #[test]
    fn the_overview_counts_the_own_account_from_the_data_folder() {
        set_thread_language(Choice::De);
        let (root, _, mut state) = real("uebersicht");
        let (shown, _) = draw(&mut state, Page::Overview, 900.0);
        // Own play time 2 h + 1 h + half an hour, the stranger's half hour is out: 3 h 30 min.
        assert!(has(&shown, "3 h 30 min"), "{shown:?}");
        // Contracts 4 + 3 = 7 (the stranger's 50 are out), jumps 10 + 6 = 16, three deaths, two ships lost.
        for (title, value) in
            [("Aufträge abgeschlossen", "7"), ("Quantensprünge", "16"), ("Tode", "3"), ("Schiffe verloren", "2")]
        {
            let at = shown.iter().position(|text| text == title).unwrap_or_else(|| panic!("{title} in {shown:?}"));
            assert_eq!(shown[at + 1], value, "{title}");
        }
        assert!(has(&shown, "1 fehlgeschlagen"), "{shown:?}");
        assert!(has(&shown, "am häufigsten: AEGS Sabre"), "no name file: the key, readable");
        for sample in ["Gladius", "128", "41 h"] {
            assert!(!has(&shown, sample), "made-up {sample}");
        }
        // The heat map has its week days and its hour numbers.
        assert!(has(&shown, "Mo") && has(&shown, "So") && shown.iter().any(|t| t == "21"), "{shown:?}");
        let _ = std::fs::remove_dir_all(root);
    }

    /// A journal of three sessions with one purchase each: `(start, [(time, price)])`.
    fn write_journal(paths: &Paths, sessions: &[(i64, Vec<(i64, f64)>)]) {
        use versekit_core::stock_journal::Purchase;
        let entries: Vec<(String, Value)> = sessions
            .iter()
            .map(|(start, buys)| {
                let bought = buys
                    .iter()
                    .map(|(time, price)| {
                        Purchase {
                            time: *time,
                            class: "x_class".into(),
                            guid: "guid".into(),
                            quantity: 1,
                            price_total: *price,
                            place: None,
                            shop: "SCShop_Kiosk".into(),
                        }
                        .to_value()
                    })
                    .collect();
                let session = vec![
                    ("von".to_string(), Value::int(*start)),
                    ("bis".to_string(), Value::int(start + 60)),
                    ("kaeufe".to_string(), Value::Array(bought)),
                    ("bewegungen".to_string(), Value::Array(Vec::new())),
                ];
                (start.to_string(), Value::Object(session))
            })
            .collect();
        let mut data =
            vec![("sitzungen".to_string(), Value::Object(entries)), ("gelesen".to_string(), Value::Object(Vec::new()))];
        assert!(stock_journal::save(paths, &mut data));
    }

    #[test]
    fn the_overview_adds_up_what_was_spent_in_the_shops_and_hides_it_while_the_reading_is_off() {
        set_thread_language(Choice::De);
        let (root, paths) = world("ausgaben");
        fill(&paths);
        let now = now_seconds() as i64;
        let day = 86_400;
        write_journal(
            &paths,
            &[
                (now - 30 * day, vec![(now - 30 * day + 5, 70.0)]),
                (now - 3 * day, vec![(now - 3 * day + 5, 200.0)]),
                (now - 100, vec![(now, 1000.0)]),
            ],
        );
        let mut state = StatsState::of(&paths);
        draw(&mut state, Page::Overview, 900.0);
        state.wait();
        let (shown, _) = draw(&mut state, Page::Overview, 900.0);
        assert!(has(&shown, "Ausgegeben im Laden"), "{shown:?}");
        let tile = |title: &str| shown[shown.iter().position(|t| t == title).expect(title) + 1].clone();
        assert_eq!(tile("Heute"), "1.000 aUEC", "{shown:?}");
        assert_eq!(tile("Letzte 7 Tage"), "1.200 aUEC");
        assert_eq!(tile("Letzte Sitzung"), "1.000 aUEC");
        assert_eq!(tile("Gesamt"), "1.270 aUEC");
        assert!(has(&shown, "Käufe: 3"), "{shown:?}");
        assert!(has(&shown, "Verkäufe zählen hier nicht mit"), "no income is claimed: {shown:?}");
        // The switch off: the section is gone, the journal stays on disk.
        assert!(stock_journal::set_enabled(&paths, false));
        let mut state = StatsState::of(&paths);
        draw(&mut state, Page::Overview, 900.0);
        state.wait();
        let (shown, _) = draw(&mut state, Page::Overview, 900.0);
        assert!(!has(&shown, "Ausgegeben im Laden"), "{shown:?}");
        assert!(std::path::Path::new(&paths.app_file(stock_journal::FILE)).exists());
        // Nothing bought: no section either.
        assert!(stock_journal::set_enabled(&paths, true));
        write_journal(&paths, &[(now - 100, Vec::new())]);
        let mut state = StatsState::of(&paths);
        draw(&mut state, Page::Overview, 900.0);
        state.wait();
        let (shown, _) = draw(&mut state, Page::Overview, 900.0);
        assert!(!has(&shown, "Ausgegeben im Laden"), "{shown:?}");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn the_ship_page_lists_ships_losses_and_weapons_with_their_share() {
        set_thread_language(Choice::De);
        let (root, _, mut state) = real("schiffe");
        let (shown, _) = draw(&mut state, Page::Ships, 900.0);
        // The most flown ship was in 2 of 3 sessions with a ship; 3 ships of the own account.
        let tile = |title: &str| shown[shown.iter().position(|t| t == title).expect(title) + 1].clone();
        assert_eq!(tile("Häufigstes Schiff"), "AEGS Sabre");
        assert_eq!(tile("Sitzungen damit"), "2");
        assert_eq!(tile("Anteil"), "67 %");
        assert_eq!(tile("Schiffe insgesamt"), "3", "the stranger's ship is not counted");
        assert!(has(&shown, "DRAK Cutlass Black") && !has(&shown, "ORIG 300i"), "{shown:?}");
        assert!(has(&shown, "2×"), "the Sabre in two sessions");
        assert!(has(&shown, "behr rifle") && has(&shown, "100 %"), "the only rifle is all of its place");
        assert!(has(&shown, "Noch nichts erfasst"), "an empty place says so");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn the_contract_page_takes_figures_from_the_statistics_and_names_from_the_log() {
        set_thread_language(Choice::De);
        let (root, _, mut state) = real("auftraege");
        let (shown, _) = draw(&mut state, Page::Missions, 900.0);
        let tile = |title: &str| shown[shown.iter().position(|t| t == title).expect(title) + 1].clone();
        assert_eq!(
            (tile("Abgeschlossen"), tile("Fehlgeschlagen"), tile("Abgebrochen")),
            ("7".into(), "1".into(), "2".into())
        );
        // 7 of 10 ended well.
        assert!(has(&shown, "70 % aller beendeten") && has(&shown, "20 % aller beendeten"), "{shown:?}");
        assert!(has(&shown, "Retake the Outpost") && has(&shown, "2×"), "the name twice");
        assert!(!has(&shown, "Deliver Supplies"), "a running contract is not listed");
        assert!(has(&shown, "abgeschlossen") && has(&shown, "fehlgeschlagen"), "{shown:?}");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn the_quantum_page_makes_places_readable_and_counts_the_arrival_rate() {
        set_thread_language(Choice::De);
        let (root, _, mut state) = real("quantum");
        let (shown, _) = draw(&mut state, Page::Quantum, 900.0);
        let tile = |title: &str| shown[shown.iter().position(|t| t == title).expect(title) + 1].clone();
        // 16 arrivals of 20 choices.
        assert_eq!(
            (tile("Angekommen"), tile("Zielwahlen"), tile("Ankunftsquote")),
            ("16".into(), "20".into(), "80 %".into())
        );
        assert!(has(&shown, "Raststation im Orbit von Pyro6"), "{shown:?}");
        assert!(has(&shown, "Eigener Wegpunkt"));
        assert!(has(&shown, "Stanton") && has(&shown, "3×"), "the start point stands as the game writes it");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn opening_a_page_looks_for_the_names_and_shows_saved_place_names() {
        set_thread_language(Choice::De);
        let (root, paths) = world("namen");
        fill(&paths);
        // In the tests the look reads no archive and writes nothing.
        let before = NAME_LOOKS.load(std::sync::atomic::Ordering::SeqCst);
        let mut state = StatsState::of(&paths);
        draw(&mut state, Page::Quantum, 900.0);
        state.wait();
        assert!(NAME_LOOKS.load(std::sync::atomic::Ordering::SeqCst) > before, "the run looks for the names");
        assert!(!std::path::Path::new(&paths.app_file(versekit_gamedata::game_text::PLACES_FILE)).exists());
        // Saved place names: the next run shows them.
        write(&paths, versekit_gamedata::game_text::PLACES_FILE, r#"{"pyro6": "Terminus"}"#);
        state.start(Job::Refresh, &egui::Context::default(), 1.0e9);
        state.wait();
        let (shown, _) = draw(&mut state, Page::Quantum, 900.0);
        assert!(has(&shown, "Raststation im Orbit von Terminus"), "{shown:?}");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn the_stability_page_lets_a_crash_beat_a_clean_end() {
        set_thread_language(Choice::De);
        let (root, _, mut state) = real("stabil");
        let (shown, _) = draw(&mut state, Page::Stability, 900.0);
        let tile = |title: &str| shown[shown.iter().position(|t| t == title).expect(title) + 1].clone();
        assert_eq!(
            (tile("Sauber beendet"), tile("Abstürze"), tile("Ohne sauberes Ende")),
            ("2".into(), "1".into(), "0".into())
        );
        assert!(has(&shown, "67 % aller Sitzungen") && has(&shown, "33 % aller Sitzungen"), "{shown:?}");
        // The reasons: a known one in words, with the sum of all at the section.
        assert!(has(&shown, "Verbindung aufgeräumt") && has(&shown, "Timeout"), "{shown:?}");
        assert!(has(&shown, "Absturz") && has(&shown, "Sauber beendet"), "the table");
        // The newest session was evaluated before the joins were kept: it says it knows no shard.
        assert!(has(&shown, "Shard der letzten Sitzung") && has(&shown, "kein Shard bekannt"), "{shown:?}");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn the_stability_page_names_the_shards_of_the_last_session_and_covers_their_numbers_in_streamer_mode() {
        set_thread_language(Choice::De);
        let (root, paths) = world("shard");
        fill(&paths);
        let file = paths.app_file("statistik.json");
        let text = std::fs::read_to_string(&file).unwrap();
        let newest = START + 300_000;
        let joins = format!(
            r#""abbrueche": {{}}, "shards": [[{}, "pub_euw1b_12881860_420"], [{}, "pub_euw1b_12881999_17"]]}}
}}}}"#,
            newest + 60,
            newest + 900
        );
        let with = text.replacen("\"abbrueche\": {}}\n}}", &joins, 1);
        assert_ne!(with, text, "the newest session gets its joins");
        std::fs::write(&file, with).unwrap();
        let mut state = StatsState::of(&paths);
        draw(&mut state, Page::Stability, 900.0);
        state.wait();
        let (shown, overflow) = draw(&mut state, Page::Stability, 900.0);
        assert!(has(&shown, "Shard der letzten Sitzung") && has(&shown, "Wechsel: 1"), "{shown:?}");
        assert!(
            has(&shown, "EU-West (euw1b)") && has(&shown, "12881860-420") && has(&shown, "12881999-17"),
            "{shown:?}"
        );
        assert!(!has(&shown, "kein Shard bekannt"), "{shown:?}");
        assert_eq!(overflow, 0.0);
        // In streamer mode the numbers are covered, the region stays.
        versekit_core::streamer::set_thread_active(Some(true));
        let (covered, _) = draw(&mut state, Page::Stability, 900.0);
        versekit_core::streamer::set_thread_active(None);
        assert!(!has(&covered, "12881860") && has(&covered, "EU-West (euw1b)"), "{covered:?}");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn the_overview_tells_the_state_of_the_evaluation_and_a_run_finds_a_new_session() {
        set_thread_language(Choice::De);
        let (root, paths, mut state) = real("auswertung");
        // Entering the page starts a reading of the running log; let it end first.
        draw(&mut state, Page::Overview, 900.0);
        state.wait();
        let (shown, _) = draw(&mut state, Page::Overview, 900.0);
        let tile = |title: &str| shown[shown.iter().position(|t| t == title).expect(title) + 1].clone();
        // Three sessions; four logs read: the three of the file and the running one, which the page reads on opening.
        assert_eq!((tile("Erfasste Sitzungen"), tile("Gelesene Protokolle")), ("4".into(), "5".into()));
        assert!(tile("Größe der Statistik-Datei").ends_with(" KB"));
        for button in ["Jetzt auswerten", "Alles neu auswerten", "Statistik speichern …"] {
            assert!(has(&shown, button), "{button}");
        }
        assert!(has(&shown, "Automatisch auswerten"), "the fixed row stays, without a switch");
        // A log with a session turns up: "Jetzt auswerten" finds it.
        let game = paths.game_folder().expect("game folder");
        let log = format!(
            "<{}> Start OnClientSpawned\n<{}> <SystemQuit>\n",
            "2026-10-05T10:00:00.000Z", "2026-10-05T11:00:00.000Z"
        );
        std::fs::write(paths.host.flavor.join(&game, "Game.log"), log).unwrap();
        assert!(state.start(Job::Evaluate, &egui::Context::default(), 0.0));
        state.wait();
        assert_eq!(state.take_notice().as_deref(), Some("Ausgewertet: 1 neue Sitzung."));
        let (shown, _) = draw(&mut state, Page::Overview, 900.0);
        let tile = |title: &str| shown[shown.iter().position(|t| t == title).expect(title) + 1].clone();
        assert_eq!((tile("Erfasste Sitzungen"), tile("Gelesene Protokolle")), ("5".into(), "5".into()));
        // "Alles neu" reads the log there is once more and finds nothing new; the sessions stay,
        // the read position of the logs long gone does not (only the log there is is known again).
        assert!(state.start(Job::Rescan, &egui::Context::default(), 0.0));
        state.wait();
        assert_eq!(state.take_notice().as_deref(), Some("Ausgewertet: 0 neue Sitzungen."));
        let (shown, _) = draw(&mut state, Page::Overview, 900.0);
        let tile = |title: &str| shown[shown.iter().position(|t| t == title).expect(title) + 1].clone();
        assert_eq!((tile("Erfasste Sitzungen"), tile("Gelesene Protokolle")), ("5".into(), "1".into()));
        let _ = std::fs::remove_dir_all(root);
    }

    /// The texts of the ten cards of the merged page, in the order they stand in: first the state of the
    /// evaluation, then the figures of the play time.
    const CARD_KEYS: [&str; 10] = [
        "s_sa_zuletzt",
        "s_sa_sitzungen",
        "s_sa_dateien",
        "s_sa_groesse",
        "s_sx_spielzeit",
        "s_sx_sitzungen",
        "s_sx_auftraege",
        "s_sx_spruenge",
        "s_sx_tode",
        "s_sx_verloren",
    ];

    /// Where the text `text` stands in a drawing (it must be there once as a whole text).
    fn place_of(shown: &[(String, Pos2)], text: &str) -> Pos2 {
        let found: Vec<Pos2> = shown.iter().filter(|(shown, _)| shown == text).map(|(_, at)| *at).collect();
        assert_eq!(found.len(), 1, "{text:?} stands {} times in {shown:?}", found.len());
        found[0]
    }

    #[test]
    fn the_overview_holds_what_both_old_pages_showed_the_cards_on_top_and_the_buttons_below() {
        // The old page key leads to the merged page.
        assert_eq!(Page::of("hf_st_auswertung"), Some(Page::Overview));
        assert_eq!(Page::of("hf_st_uebersicht"), Some(Page::Overview));
        for choice in [Choice::De, Choice::En] {
            set_thread_language(choice);
            let (root, _, mut state) = real("vereint");
            let (shown, _) = draw_placed(&mut state, Page::Overview, 900.0, "original");
            let cards: Vec<Pos2> = CARD_KEYS.iter().map(|key| place_of(&shown, &t(key))).collect();
            let last_card_row = cards.iter().map(|at| at.y).fold(f32::MIN, f32::max);
            let heat = place_of(&shown, &t("s_sx_wann"));
            let evaluation = place_of(&shown, &t("hf_st_auswertung"));
            assert!(heat.y > last_card_row, "{choice:?}: the heat map under the cards");
            assert!(evaluation.y > heat.y, "{choice:?}: the evaluation under the heat map");
            // The buttons, the row of the automatic evaluation and the row of the export, then the notes.
            let below = [
                place_of(&shown, &t("s_sa_jetzt")),
                place_of(&shown, &t("s_sa_neu")),
                place_of(&shown, &t("s_sa_auto")),
                place_of(&shown, t("s_sx_export").trim_end_matches([' ', '…'])),
                place_of(&shown, &t("s_sx_zahlen")),
            ];
            assert!(
                below.iter().all(|at| at.y > evaluation.y),
                "{choice:?}: all of it under the head of the evaluation"
            );
            assert!(
                below[0].y < below[2].y && below[2].y < below[3].y && below[3].y < below[4].y,
                "{choice:?}: {below:?}"
            );
            assert!(
                has(&shown.iter().map(|(text, _)| text.clone()).collect::<Vec<_>>(), &t("s_sx_export")),
                "the button"
            );
            assert!(shown.iter().any(|(text, _)| *text == t("s_sx_lead")), "{choice:?}: one lead on top");
            let _ = std::fs::remove_dir_all(root);
        }
        set_thread_language(Choice::Auto);
    }

    /// How many of the cards stand on each line of the page, from the top.
    fn card_rows(shown: &[(String, Pos2)]) -> Vec<Vec<f32>> {
        let mut rows: Vec<Vec<f32>> = Vec::new();
        let mut row_y = f32::MIN;
        for key in CARD_KEYS {
            let at = place_of(shown, &t(key));
            if rows.is_empty() || (at.y - row_y).abs() > 1.0 {
                rows.push(Vec::new());
                row_y = at.y;
            }
            rows.last_mut().unwrap().push(at.x);
        }
        rows
    }

    #[test]
    fn the_ten_cards_stand_five_and_five_or_two_by_two_all_of_one_width_and_never_stick_out() {
        let (root, _, mut state) = real("karten");
        for choice in [Choice::De, Choice::En] {
            set_thread_language(choice);
            // Width of the page, columns: five side by side from 5 × 170 + 4 × 8, two from 2 × 170 + 8, else one.
            for (width, columns) in [(1500.0, 5), (1000.0, 5), (700.0, 2), (420.0, 2), (330.0, 1)] {
                let (shown, overflow) = draw_placed(&mut state, Page::Overview, width, "original");
                let rows = card_rows(&shown);
                let what = format!("{choice:?} {width}");
                assert_eq!(rows.len(), 10 / columns, "{what}: {rows:?}");
                for row in &rows {
                    assert_eq!(row.len(), columns, "{what}: a row is never half empty: {rows:?}");
                    // Equal widths: the same step from card to card, and the same columns in every row.
                    for pair in row.windows(2) {
                        assert!((pair[1] - pair[0] - (row[1] - row[0])).abs() < 1.0, "{what}: {row:?}");
                    }
                    assert_eq!(
                        row.iter().map(|x| x.round()).collect::<Vec<_>>(),
                        rows[0].iter().map(|x| x.round()).collect::<Vec<_>>()
                    );
                }
                assert!(overflow < 0.5, "{what}: sticks out by {overflow}");
            }
        }
        set_thread_language(Choice::Auto);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn without_any_session_the_overview_keeps_the_state_of_the_evaluation_and_its_buttons() {
        let (root, paths) = world("leer-vereint");
        for choice in [Choice::De, Choice::En] {
            set_thread_language(choice);
            let mut state = StatsState::of(&paths);
            draw(&mut state, Page::Overview, 900.0);
            state.wait();
            for width in [900.0, 420.0] {
                let (shown, overflow) = draw_placed(&mut state, Page::Overview, width, "original");
                let texts: Vec<String> = shown.iter().map(|(text, _)| text.clone()).collect();
                // (whole texts: "Sitzungen" is also part of "Erfasste Sitzungen")
                let is_shown = |key: &str| texts.iter().any(|text| *text == t(key));
                for key in &CARD_KEYS[..4] {
                    assert!(is_shown(key), "{choice:?} {width}: {key}");
                }
                for key in &CARD_KEYS[4..] {
                    assert!(!is_shown(key), "{choice:?} {width}: no figure of the play time without a session");
                }
                assert!(has(&texts, &t("s_sx_leer")), "{choice:?}: says that nothing is recorded");
                for key in ["s_sa_jetzt", "s_sa_neu", "s_sa_auto", "s_sx_export", "s_sx_zahlen"] {
                    assert!(has(&texts, &t(key)), "{choice:?} {width}: {key}");
                }
                assert!(overflow < 0.5, "{choice:?} {width}: sticks out by {overflow}");
            }
        }
        set_thread_language(Choice::Auto);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn the_export_holds_the_figures_and_not_the_player() {
        set_thread_language(Choice::De);
        let (root, _, mut state) = real("export");
        let target = root.join("statistik.json");
        state.write_export(target.to_str().unwrap());
        assert_eq!(state.take_notice().as_deref(), Some("Gespeichert: statistik.json"));
        let text = std::fs::read_to_string(&target).unwrap();
        assert!(!text.contains("Xharig") && !text.contains("Fremder"), "no account in the file");
        let value = versekit_core::jsonio::loads(&text).unwrap();
        let total = value.get("gesamt").unwrap();
        assert_eq!(total.get("auftraege"), Some(&Value::int(7)));
        assert_eq!(total.get("spielzeit"), Some(&Value::int(3 * 3600 + 1800)));
        // An unwritable target says so.
        state.write_export(root.join("gibt-es-nicht").join("x.json").to_str().unwrap());
        assert_eq!(state.take_notice().as_deref(), Some("Die Statistik ließ sich nicht speichern."));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn without_data_the_pages_say_what_to_do_and_show_no_made_up_figures() {
        set_thread_language(Choice::De);
        let (root, paths) = world("leer");
        let mut state = StatsState::of(&paths);
        draw(&mut state, Page::Overview, 900.0);
        state.wait();
        let (shown, _) = draw(&mut state, Page::Overview, 900.0);
        assert!(has(&shown, "Noch keine Sitzung gefunden"), "{shown:?}");
        for page in [Page::Ships, Page::Missions, Page::Quantum, Page::Stability] {
            let (shown, _) = draw(&mut state, page, 900.0);
            assert!(has(&shown, "Noch nichts erfasst"), "{page:?} {shown:?}");
            for sample in ["Gladius", "128", "Pyro6", "Retake the Outpost"] {
                assert!(!has(&shown, sample), "{page:?}: made-up {sample}");
            }
        }
        let _ = std::fs::remove_dir_all(root);
        // Before any look has run, the state says so: never, and no file yet.
        let (root, paths) = world("nie");
        let mut state = StatsState::sample();
        state.report = Some(Arc::new(Report::build(&paths, None, None)));
        let (shown, _) = draw(&mut state, Page::Overview, 900.0);
        assert!(has(&shown, "noch nie"), "never evaluated: {shown:?}");
        assert!(shown.iter().any(|text| text == NONE), "no file yet");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn the_sample_page_shows_sample_figures_and_reads_nothing() {
        set_thread_language(Choice::De);
        let mut state = StatsState::sample();
        let (shown, _) = draw(&mut state, Page::Ships, 900.0);
        assert!(has(&shown, "AEGS Gladius"), "{shown:?}");
        assert!(state.is_sample() && !state.running());
        // A click on "Jetzt auswerten" answers at once, with a made-up count.
        assert!(state.start(Job::Evaluate, &egui::Context::default(), 0.0));
        assert_eq!(state.take_notice().as_deref(), Some("Ausgewertet: 3 neue Sitzungen."));
    }

    #[test]
    fn no_page_sticks_out_in_either_language_at_any_width() {
        let (root, _, mut state) = real("breite");
        let mut findings = Vec::new();
        for choice in [Choice::De, Choice::En] {
            set_thread_language(choice);
            for scheme in theme::schemes() {
                for width in [420.0, 640.0, 900.0, 1500.0] {
                    for page in [Page::Overview, Page::Ships, Page::Missions, Page::Quantum, Page::Stability] {
                        let (_, overflow) = draw_in(&mut state, page, width, scheme.id);
                        if overflow > 0.5 {
                            findings.push(format!("{choice:?} {} {page:?} {width}: {overflow:.0}", scheme.id));
                        }
                    }
                }
            }
        }
        set_thread_language(Choice::Auto);
        assert!(findings.is_empty(), "{findings:?}");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn while_a_run_is_on_the_buttons_wait_and_say_why() {
        set_thread_language(Choice::De);
        let (root, _, mut state) = real("gesperrt");
        // Entering the page starts a reading of the running log; let it end first.
        draw(&mut state, Page::Overview, 900.0);
        state.wait();
        let (shown, _) = draw(&mut state, Page::Overview, 900.0);
        assert!(!has(&shown, "Wird ausgewertet …"), "idle: no waiting line");
        // A run that goes on (set by hand, so that the test does not race with a thread).
        state.shared.lock().unwrap().running = true;
        let (shown, _) = draw(&mut state, Page::Overview, 900.0);
        assert!(has(&shown, "Wird ausgewertet …"), "{shown:?}");
        state.shared.lock().unwrap().running = false;
        let (shown, _) = draw(&mut state, Page::Overview, 900.0);
        assert!(!has(&shown, "Wird ausgewertet …"), "over again");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn the_ranking_bar_text_is_readable_in_every_scheme() {
        let mut low = Vec::new();
        for scheme in theme::schemes() {
            for tone in [RankTone::Accent, RankTone::Red, RankTone::Gold] {
                let ratio = scheme.fg.contrast(widgets::rank_bar(scheme, tone));
                if ratio < 4.5 {
                    low.push(format!("{} {tone:?}: {ratio:.2}", scheme.id));
                }
            }
        }
        assert!(low.is_empty(), "{low:?}");
    }
}
