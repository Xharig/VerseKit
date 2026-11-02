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

//! The contracts and log page: which contract was played when and how
//! it ended, a list and nothing else. Search, state toggles with counts,
//! per row date and state in fixed columns, name, objectives and blueprints;
//! a click opens the row with times, source, how often, and the jump to its
//! blueprints; the bug (on hovering the row) searches the contract in the
//! Issue Council.

use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use egui::text::LayoutJob;
use egui::{Align, Layout, RichText, Sense, TextFormat, Ui, UiBuilder, Vec2};
use versekit_core::browser;
use versekit_core::catalog_cache::CatalogCache;
use versekit_core::errors;
use versekit_core::issue_council::{self, Search};
use versekit_core::jsonio::Map;
use versekit_core::language::{Arg, t, tf};
use versekit_core::logsource::LogSource;
use versekit_core::mission_log::{self as log, ContractBlueprints, Mission};
use versekit_core::mission_scan::MissionScan;
use versekit_core::paths::Paths;
use versekit_core::text::py_strip;
use versekit_core::theme::{Color, IconTint, Scheme};

use crate::blueprint_list::ListJump;
use crate::layout::GAP;
use crate::widgets::{self, ButtonKind};
use crate::{color, icons};

/// The state toggles in the order of the outcome: all, running, done, what
/// went wrong, the uncertain last.
const STATES: [Option<&str>; 6] =
    [None, Some(log::RUNNING), Some(log::COMPLETED), Some(log::ABORTED), Some(log::FAILED), Some(log::EXPIRED)];
/// How often the game's running state is looked up again, in seconds.
const RUNNING_CHECK_SEC: f64 = 5.0;
/// Room of the bug at the right of a row, also while it is hidden.
const BUG_SLOT: f32 = widgets::ICON_BUTTON_SIZE;

/// What needs the language files of the installation: two files of ten
/// megabytes, so built once in the background, when the page is first shown.
pub struct Lookups {
    pub blueprints: ContractBlueprints,
    pub search: Search,
}

/// Where the language files come from.
enum Languages {
    /// The installation's, from the game folder of the settings.
    Installation,
    /// These files (tests, or none at all).
    Files(Vec<PathBuf>),
}

/// Where the reading of the game logs stands.
enum ScanSlot {
    Idle,
    Running,
    /// Finished: the saved log as it is now, not yet taken over.
    Done(Vec<Map>),
}

/// Everything the page shows, read once, and refreshed when the logs have
/// been read again.
pub struct LogData {
    pub entries: Vec<Map>,
    pub missions: Vec<Mission>,
    /// Name → (played, completed).
    pub played: HashMap<String, (usize, usize)>,
    /// True for sample contents (nothing of the user's).
    pub sample: bool,
    paths: Option<Paths>,
    languages: Arc<Languages>,
    lookups: Arc<OnceLock<Lookups>>,
    started: AtomicBool,
    scan: Arc<Mutex<ScanSlot>>,
    /// The reader of the game logs, built by the first reading.
    reader: Arc<OnceLock<(&'static Paths, &'static MissionScan<'static, 'static>)>>,
}

impl LogData {
    fn of(entries: Vec<Map>, paths: Option<Paths>) -> LogData {
        LogData {
            missions: entries.iter().map(Mission::of).collect(),
            played: log::summarize(&entries).into_iter().collect(),
            entries,
            sample: false,
            paths,
            languages: Arc::new(Languages::Installation),
            lookups: Arc::new(OnceLock::new()),
            started: AtomicBool::new(false),
            scan: Arc::new(Mutex::new(ScanSlot::Idle)),
            reader: Arc::new(OnceLock::new()),
        }
    }

    /// Reads the game logs again in the background, at the start and whenever
    /// the page is shown: a contract handed in at noon is there when the page
    /// is opened. Nothing happens for sample contents or while a reading runs.
    /// What it finds is taken over by [`LogData::poll`].
    ///
    /// ⚠ A failed reading leaves the saved log as it is: better than a page
    /// left empty.
    pub fn rescan(&self, ctx: Option<&egui::Context>) {
        let Some(paths) = &self.paths else { return };
        {
            let mut slot = self.scan.lock().unwrap_or_else(|e| e.into_inner());
            if !matches!(*slot, ScanSlot::Idle) {
                return;
            }
            *slot = ScanSlot::Running;
        }
        let (slot, host, ctx) = (Arc::clone(&self.scan), paths.host.clone(), ctx.cloned());
        let (reader, lock) = (Arc::clone(&self.reader), versekit_core::writeguard::state());
        std::thread::spawn(move || {
            // The slot falls back to idle even when this thread dies half way.
            struct Release(Arc<Mutex<ScanSlot>>, Option<ScanSlot>);
            impl Drop for Release {
                fn drop(&mut self) {
                    *self.0.lock().unwrap_or_else(|e| e.into_inner()) = self.1.take().unwrap_or(ScanSlot::Idle);
                }
            }
            let mut release = Release(slot, None);
            versekit_core::writeguard::adopt(&lock);
            // Built once and kept: its memory of the logs outlives every reading.
            let (paths, scan) = *reader.get_or_init(|| {
                let paths: &'static Paths = Box::leak(Box::new(Paths::new(host)));
                let src: &'static LogSource<'static> =
                    Box::leak(Box::new(LogSource::new(paths, LogSource::sources_of(paths))));
                (paths, Box::leak(Box::new(MissionScan::new(src))) as &'static MissionScan<'static, 'static>)
            });
            release.1 = match scan.scan_backlog() {
                Ok(_) => Some(ScanSlot::Done(log::load(paths))),
                Err(e) => {
                    errors::record("pages.auftragslog_nachlese", &e);
                    None
                }
            };
            drop(release);
            if let Some(ctx) = ctx {
                ctx.request_repaint();
            }
        });
    }

    /// Is a reading of the game logs running?
    pub fn scanning(&self) -> bool {
        matches!(*self.scan.lock().unwrap_or_else(|e| e.into_inner()), ScanSlot::Running)
    }

    /// Takes over what a finished reading found; true when the list changed.
    pub fn poll(&mut self) -> bool {
        let done = {
            let mut slot = self.scan.lock().unwrap_or_else(|e| e.into_inner());
            match &*slot {
                ScanSlot::Done(_) => std::mem::replace(&mut *slot, ScanSlot::Idle),
                _ => return false,
            }
        };
        let ScanSlot::Done(entries) = done else { return false };
        if entries == self.entries {
            return false;
        }
        self.missions = entries.iter().map(Mission::of).collect();
        self.played = log::summarize(&entries).into_iter().collect();
        self.entries = entries;
        true
    }

    /// The saved log of the data folder. Index and English titles follow
    /// when the page is shown.
    pub fn load(paths: &Paths) -> LogData {
        LogData::of(log::load(paths), Some(Paths::new(paths.host.clone())))
    }

    /// The same, with these language files instead of the installation's.
    pub fn with_languages(mut self, files: Vec<PathBuf>) -> LogData {
        self.languages = Arc::new(Languages::Files(files));
        self
    }

    /// Made-up contents for the preview: every state, objectives, blueprints,
    /// a long title, contracts of the sample catalog.
    pub fn sample() -> LogData {
        use versekit_core::jsonio::Value;
        let rows: [(&str, &str, &str, &[&str], i64); 12] = [
            ("Protect Fuel Tanks", "2026-10-18T16:31:14", log::RUNNING, &[], 5),
            ("Retake the Outpost", "2026-10-18T15:02:40", log::COMPLETED, &["Arclight Pistol"], 0),
            ("Deliver Supplies", "2026-10-17T21:12:03", log::ABORTED, &[], 0),
            (
                "Verified Bounty: Brendon Broad | HRT (Großes Mehrbesatzungsschiff, mittlere Unterstützung)",
                "2026-10-17T20:40:00",
                log::FAILED,
                &[],
                0,
            ),
            ("Recover Vanduul Corpses", "2026-10-16T19:00:00", log::EXPIRED, &[], 0),
            ("Retake the Outpost", "2026-10-15T18:30:00", log::COMPLETED, &["Morozov-SH Helmet", "Morozov-SH Core"], 0),
            ("Stop Rival Attack at Asteroiden Bergbaubasis", "2026-10-15T17:00:00", log::COMPLETED, &[], 0),
            ("Protect Fuel Tanks", "2026-10-14T12:00:00", log::COMPLETED, &[], 0),
            ("Deliver Supplies", "2026-10-13T11:00:00", log::COMPLETED, &[], 0),
            ("Recover Vanduul Corpses", "2026-10-12T10:00:00", log::FAILED, &[], 0),
            ("Retake the Outpost", "2026-10-11T09:00:00", log::ABORTED, &[], 0),
            ("Deliver Supplies", "2026-10-10T08:00:00", log::EXPIRED, &[], 0),
        ];
        let entries: Vec<Map> = rows
            .iter()
            .map(|(name, when, state, blueprints, goals)| {
                let mut map: Map = vec![
                    ("name".into(), Value::str(*name)),
                    ("wann".into(), Value::str(*when)),
                    ("zustand".into(), Value::str(*state)),
                    ("ziele_fertig".into(), Value::int(if *goals > 0 { 2 } else { 0 })),
                    ("ziele_gesamt".into(), Value::int(*goals)),
                    ("bauplaene".into(), Value::Array(blueprints.iter().map(|b| Value::str(*b)).collect())),
                    ("quelle".into(), Value::str("Game Build(12913470) 18 Oct 26 (15 22 07).log")),
                ];
                if *state != log::RUNNING && *state != log::EXPIRED {
                    map.push(("bis".into(), Value::str(when.replace("T1", "T2"))));
                }
                map
            })
            .collect();
        let mut data = LogData::of(entries, None);
        data.sample = true;
        data
    }

    /// Starts building index and English titles in the background, once.
    pub fn start_lookups(&self) {
        if self.started.swap(true, Ordering::Relaxed) {
            return;
        }
        let Some(paths) = &self.paths else {
            let _ = self.lookups.set(Lookups {
                blueprints: ContractBlueprints::default(),
                search: Search::new(issue_council::Originals::default(), Vec::new()),
            });
            return;
        };
        let (lookups, languages, host) = (Arc::clone(&self.lookups), Arc::clone(&self.languages), paths.host.clone());
        std::thread::spawn(move || {
            let paths = Paths::new(host);
            let files = match &*languages {
                Languages::Installation => log::ini_files(&paths),
                Languages::Files(files) => files.clone(),
            };
            let _ = lookups.set(Lookups {
                blueprints: ContractBlueprints::with_ini(&paths, &files),
                search: Search::with_ini(&paths, &files),
            });
        });
    }

    /// Index and English titles once they are built; never waits.
    pub fn lookups(&self) -> Option<&Lookups> {
        self.start_lookups();
        self.lookups.get()
    }

    /// Is the game writing its log right now? The sample's game is closed.
    fn game_running(&self) -> bool {
        self.paths.as_ref().is_some_and(log::game_running)
    }
}

/// What the page is set to.
#[derive(Default)]
pub struct LogState {
    pub search: String,
    /// Index into `STATES`.
    pub state: usize,
    open: Option<usize>,
    detail_height: f32,
    to_top: bool,
    /// When the game's running state was looked up, and what it said.
    running: Option<(f64, bool)>,
    /// The jump of the open row, worked out once, and that row.
    jump: Option<(usize, Option<Jump>)>,
    /// An address the browser did not open, shown with the page.
    not_opened: Option<String>,
    /// How far the widest drawn row stuck out of its place (the list clips it unseen).
    pub row_overflow: f32,
    /// What the last frame showed: toggle words with their counts, and the rows.
    pub(crate) drawn: (Vec<(String, usize)>, Vec<usize>),
    /// The frame the page was drawn in last: a gap means the page was left and entered again.
    last_frame: Option<u64>,
}

#[cfg(test)]
thread_local! {
    /// Counter-checks of the edge test: the open row, or the head, gets something too wide.
    static TOO_WIDE_ROW: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static TOO_WIDE_HEAD: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

impl LogState {
    /// Works the jump of the open row out anew: the catalog it was worked
    /// out from has been replaced.
    pub fn forget_jump(&mut self) {
        self.jump = None;
    }

    /// Opens the first row the last frame showed, as a click on it does. Whether there was one.
    pub fn choose_first(&mut self) -> bool {
        self.open = self.drawn.1.first().copied();
        self.open.is_some()
    }

    /// Search or a state toggle narrows the list.
    pub fn is_narrowed(&self) -> bool {
        !py_strip(&self.search).is_empty() || self.state != 0
    }

    /// Back to all contracts; the open row stays open.
    pub fn reset(&mut self) {
        *self = LogState {
            open: self.open,
            jump: self.jump.take(),
            running: self.running,
            last_frame: self.last_frame,
            to_top: true,
            ..LogState::default()
        };
    }
}

/// Color of a state, from the scheme: green for what was achieved, pale red
/// for what went wrong, grey for what nothing is said about.
fn state_color(scheme: &Scheme, state: &str) -> Color {
    match state {
        log::COMPLETED => scheme.accent,
        log::ABORTED | log::FAILED => scheme.red_pale,
        log::RUNNING => scheme.gold,
        _ => scheme.sub,
    }
}

fn state_icon(state: &str) -> &'static str {
    match state {
        log::COMPLETED => "circle-check",
        log::ABORTED => "ban",
        log::FAILED => "circle-x",
        log::RUNNING => "circle-play",
        _ => "circle-dashed",
    }
}

/// A jump into the blueprint list: the contract's title and the keys of its blueprints.
pub type Jump = (String, BTreeSet<String>);

/// Where the blueprint list goes for this contract; `None` when the catalog
/// knows no blueprint of it.
pub fn jump_for(lookups: &Lookups, catalog: &CatalogCache, name: &str) -> Option<Jump> {
    let keys = lookups.blueprints.for_contract(catalog, name);
    (!keys.is_empty()).then(|| (py_strip(name).to_string(), keys))
}

/// What a click on the page asks for.
pub enum LogAction {
    /// The blueprint list on this contract, with where the jump came from.
    Jump(ListJump, String),
}

fn galley_width(ui: &Ui, text: &str) -> f32 {
    egui::WidgetText::from(RichText::new(text).small())
        .into_galley(ui, Some(egui::TextWrapMode::Extend), f32::INFINITY, egui::TextStyle::Small)
        .size()
        .x
}

/// Opens an address; when the browser does not, the page shows it.
fn open(state: &mut LogState, url: String) {
    state.not_opened = (!browser::open_in_browser(&url)).then_some(url);
}

/// Draws the page; returns a jump into the blueprint list.
pub fn show(
    ui: &mut Ui,
    scheme: &Scheme,
    data: &mut LogData,
    catalog: &CatalogCache,
    state: &mut LogState,
) -> Option<LogAction> {
    // What a finished reading of the game logs found. The rows are numbered by
    // place: an open row is found again by name and start, its jump is worked
    // out anew.
    let open_row = state.open.and_then(|i| data.missions.get(i)).map(|m| (m.name.clone(), m.when.clone()));
    if data.poll() {
        state.open =
            open_row.and_then(|(name, when)| data.missions.iter().position(|m| m.name == name && m.when == when));
        state.jump = None;
    }
    // Entering the page reads the logs again; a reading in flight is waited for.
    let frame = ui.ctx().cumulative_frame_nr();
    if state.last_frame.is_none_or(|last| frame > last + 1) {
        data.rescan(Some(ui.ctx()));
    }
    state.last_frame = Some(frame);
    if data.scanning() {
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(250));
    }
    let data = &*data;
    data.start_lookups();
    ui.label(RichText::new(t("s_al_lead")).small().color(color(scheme.sub)));
    ui.label(RichText::new(t("s_al_hinweis")).small().color(color(scheme.sub)));
    if widgets::external_link(ui, scheme, &t("s_al_spectrum")).clicked() {
        open(state, issue_council::SPECTRUM_URL.to_string());
    }
    if let Some(url) = &state.not_opened {
        let said = tf("s_ub_auf_nein", &[("0", Arg::from(url.as_str()))]);
        ui.add(
            egui::Label::new(RichText::new(said).small().color(color(widgets::quiet_red(scheme))))
                .wrap()
                .selectable(true),
        );
    }
    #[cfg(test)]
    if TOO_WIDE_HEAD.get() {
        ui.add_sized([ui.available_width() + 40.0, 10.0], egui::Label::new("zu breit"));
    }
    ui.add_space(4.0);

    // Asked once in a while, not per row: it looks at a file.
    let now = ui.input(|i| i.time);
    let running = match state.running {
        Some((at, said)) if now - at < RUNNING_CHECK_SEC => said,
        _ => {
            let said = data.game_running();
            state.running = Some((now, said));
            said
        }
    };

    // Nothing recorded at all is something else than a search finding nothing.
    if data.entries.is_empty() {
        state.drawn = (Vec::new(), Vec::new());
        widgets::list_frame(ui, scheme, |list| {
            list.set_width(list.available_width());
            widgets::empty_state(list, scheme, "notebook-pen", &t("s_al_leer"));
        });
        return None;
    }

    let width = ui.available_width();
    if widgets::search_field(ui, &mut state.search, &t("s_pl_auftrag"), scheme, width).changed() {
        state.to_top = true;
    }
    let counts: Vec<(String, usize)> = STATES
        .iter()
        .map(|s| {
            let word = match s {
                None => t("s_al_f_alle"),
                Some(s) => t(log::state_word(s, running)),
            };
            (word, log::shown_at(&data.entries, *s, &state.search).len())
        })
        .collect();
    let toggles: Vec<(&str, usize)> = counts.iter().map(|(w, n)| (w.as_str(), *n)).collect();
    if widgets::count_toggles(ui, scheme, &toggles, &mut state.state) {
        state.to_top = true;
    }
    let shown = log::shown_at(&data.entries, STATES[state.state.min(STATES.len() - 1)], &state.search);
    ui.label(RichText::new(tf("s_al_anzahl", &[("0", Arg::from(shown.len()))])).small().color(color(scheme.sub)));
    state.drawn = (counts, shown.clone());

    if shown.is_empty() {
        widgets::list_frame(ui, scheme, |list| {
            list.set_width(list.available_width());
            widgets::empty_state(list, scheme, "scan-search", &t("s_al_nichts"));
        });
        return None;
    }
    if state.open.is_some_and(|o| !shown.contains(&o)) {
        state.open = None;
    }
    // The open row's jump, worked out once the language files are read.
    if let Some(open) = state.open
        && state.jump.as_ref().is_none_or(|(row, _)| *row != open)
        && let Some(lookups) = data.lookups()
    {
        state.jump = Some((open, jump_for(lookups, catalog, &data.missions[open].name)));
    }

    // Fixed columns, as wide as their longest content.
    let date_width = galley_width(ui, "2026-10-18") + 2.0;
    let state_width =
        STATES.iter().flatten().map(|s| galley_width(ui, &t(log::state_word(s, running)))).fold(0.0, f32::max)
            + 14.0
            + ui.spacing().item_spacing.x
            + 2.0;
    let text_height = ui.text_style_height(&egui::TextStyle::Body);
    let small_height = ui.text_style_height(&egui::TextStyle::Small);
    let row_height = text_height + small_height + 10.0;
    let height_of = |i: usize| if state.open == Some(i) { row_height + state.detail_height } else { row_height };
    let tops: Vec<f32> = shown
        .iter()
        .scan(0.0, |y, &i| {
            let top = *y;
            *y += height_of(i);
            Some(top)
        })
        .collect();
    let total_height = tops.last().copied().unwrap_or(0.0) + shown.last().map_or(0.0, |&i| height_of(i));
    let room = crate::layout::list_room(ui, ui.text_style_height(&egui::TextStyle::Body) + 10.0);
    let ready = data.lookups().is_some();
    let mut clicked: Option<(usize, RowClick)> = None;
    let mut measured = None;
    let mut row_overflow: f32 = 0.0;
    widgets::list_frame(ui, scheme, |list| {
        list.set_width(list.available_width());
        let mut area = egui::ScrollArea::vertical()
            .id_salt("auftragslog")
            .max_height(room)
            .min_scrolled_height(room)
            .auto_shrink([false, false]);
        if std::mem::take(&mut state.to_top) {
            area = area.vertical_scroll_offset(0.0);
        }
        area.show_viewport(list, |rows, viewport| {
            rows.set_min_height(total_height);
            let origin = rows.min_rect().min;
            let width = rows.available_width();
            let first = tops.partition_point(|&top| top + row_height < viewport.min.y).saturating_sub(1);
            for (n, &i) in shown.iter().enumerate().skip(first) {
                if tops[n] > viewport.max.y {
                    break;
                }
                let rect =
                    egui::Rect::from_min_size(origin + egui::vec2(0.0, tops[n]), egui::vec2(width, height_of(i)));
                // Every second row striped.
                if n % 2 == 1 {
                    rows.painter().rect_filled(rect, 0.0, color(scheme.bar));
                }
                let mut child = rows.new_child(UiBuilder::new().max_rect(rect).layout(Layout::top_down(Align::Min)));
                let open = state.open == Some(i);
                let mission = &data.missions[i];
                // The bug only while the mouse is over the row.
                let bug = rows.rect_contains_pointer(rect).then_some(ready);
                let looks = RowLooks { open, running, bug, columns: (date_width, state_width) };
                if let Some(click) = row(&mut child, scheme, mission, &looks) {
                    clicked = Some((i, click));
                }
                if open {
                    let top = child.cursor().min.y;
                    child.indent(("auftrag-detail", i), |detail| {
                        let jump = state.jump.as_ref().filter(|(row, _)| *row == i).map(|(_, jump)| jump);
                        if let Some(click) = row_detail(detail, scheme, data, mission, jump, ready) {
                            clicked = Some((i, click));
                        }
                    });
                    measured = Some(child.cursor().min.y - top + 6.0);
                }
                row_overflow = row_overflow.max(child.min_rect().right() - rect.right());
            }
        });
    });
    state.row_overflow = row_overflow.max(0.0);
    if let Some(height) = measured
        && (height - state.detail_height).abs() > 0.5
    {
        state.detail_height = height;
        ui.ctx().request_repaint();
    }
    // Waiting for the language files: draw again until they are there.
    if !ready {
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(200));
    }
    let mut action = None;
    match clicked {
        Some((i, RowClick::Open)) => state.open = if state.open == Some(i) { None } else { Some(i) },
        Some((i, RowClick::IssueCouncil)) => {
            if let Some(lookups) = data.lookups() {
                open(state, lookups.search.url_for(&data.missions[i].name));
            }
        }
        Some((i, RowClick::Jump)) => {
            if let Some((_, Some((title, keys)))) = &state.jump {
                let from = format!("{}: {}", t("hf_auftragslog"), data.missions[i].name);
                action = Some(LogAction::Jump(ListJump::Contract(title.clone(), keys.clone()), from));
            }
        }
        None => {}
    }
    action
}

enum RowClick {
    Open,
    IssueCouncil,
    Jump,
}

/// How a row is drawn: open, the word for running, the bug (`None` hidden,
/// `Some(false)` while the language files are read), the fixed columns.
struct RowLooks {
    open: bool,
    running: bool,
    bug: Option<bool>,
    columns: (f32, f32),
}

/// One row: date and state in fixed columns, name with objectives and
/// blueprints, the bug at the right.
fn row(ui: &mut Ui, scheme: &Scheme, mission: &Mission, looks: &RowLooks) -> Option<RowClick> {
    let mut click = None;
    let tone = state_color(scheme, &mission.state);
    ui.add_space(4.0);
    ui.horizontal(|line| {
        line.add_space(4.0);
        let height = line.spacing().interact_size.y;
        line.add_sized(
            [looks.columns.0, height],
            egui::Label::new(RichText::new(mission.day()).small().color(color(scheme.sub))),
        );
        let (cell, _) = line.allocate_exact_size(Vec2::new(looks.columns.1, height), Sense::hover());
        let mut word = line.new_child(UiBuilder::new().max_rect(cell).layout(Layout::left_to_right(Align::Center)));
        if let Some(image) = icons::image(state_icon(&mission.state), tone, 14.0) {
            word.add(image);
        }
        word.label(RichText::new(t(log::state_word(&mission.state, looks.running))).small().color(color(tone)));
        line.with_layout(Layout::right_to_left(Align::Center), |right| {
            right.add_space(6.0);
            match looks.bug {
                Some(true) => {
                    if widgets::icon_button(right, scheme, "bug", &t("s_al_ic_tipp")).clicked() {
                        click = Some(RowClick::IssueCouncil);
                    }
                }
                Some(false) => {
                    let (slot, _) = right.allocate_exact_size(Vec2::splat(BUG_SLOT), Sense::hover());
                    let spinner = egui::Spinner::new().size(14.0).color(color(scheme.accent));
                    right.put(egui::Rect::from_center_size(slot.center(), Vec2::splat(14.0)), spinner);
                }
                None => {
                    right.allocate_exact_size(Vec2::splat(BUG_SLOT), Sense::hover());
                }
            }
            right.with_layout(Layout::left_to_right(Align::Center), |middle| {
                let arrow = if looks.open { "chevron-down" } else { "chevron-right" };
                if let Some(image) = icons::themed(arrow, scheme, IconTint::Normal, 12.0) {
                    middle.add(image);
                }
                middle.vertical(|names| {
                    let name = egui::Label::new(RichText::new(&mission.name).color(color(scheme.fg)))
                        .truncate()
                        .sense(Sense::click());
                    let response = names
                        .add(name)
                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                        .on_hover_text(format!("{}\n{}", mission.name, t("s_al_zeile_tipp")));
                    if response.clicked() {
                        click = Some(RowClick::Open);
                    }
                    if let Some(job) = sub_line(names, scheme, mission) {
                        names.add(egui::Label::new(job).truncate());
                    }
                });
            });
        });
    });
    click
}

/// Objectives (only while running) and blueprints (in the accent: mostly
/// why someone looks a contract up).
fn sub_line(ui: &Ui, scheme: &Scheme, mission: &Mission) -> Option<LayoutJob> {
    let font = egui::TextStyle::Small.resolve(ui.style());
    let mut job = LayoutJob::default();
    if mission.shows_goals() {
        let goals = tf("s_al_ziele", &[("0", Arg::from(mission.goals_done)), ("1", Arg::from(mission.goals_total))]);
        job.append(&goals, 0.0, TextFormat::simple(font.clone(), color(scheme.sub)));
    }
    if !mission.blueprints.is_empty() {
        if !job.text.is_empty() {
            job.append(" · ", 0.0, TextFormat::simple(font.clone(), color(scheme.sub)));
        }
        job.append(&blueprint_text(mission), 0.0, TextFormat::simple(font, color(scheme.accent)));
    }
    (!job.text.is_empty()).then_some(job)
}

fn blueprint_text(mission: &Mission) -> String {
    let key = if mission.blueprints.len() == 1 { "s_al_bp" } else { "s_al_bp_mehr" };
    tf(key, &[("0", Arg::from(mission.blueprints.join(" · ")))])
}

/// The open row: full name, times, log file, how often, and what to do.
/// `jump` is the row's worked-out jump, `None` while it is not known yet.
fn row_detail(
    ui: &mut Ui,
    scheme: &Scheme,
    data: &LogData,
    mission: &Mission,
    jump: Option<&Option<Jump>>,
    ready: bool,
) -> Option<RowClick> {
    let mut click = None;
    let small = |text: String| RichText::new(text).small().color(color(scheme.sub));
    ui.add(egui::Label::new(RichText::new(&mission.name).strong()).wrap());
    let mut facts = vec![tf("s_al_angenommen", &[("0", Arg::from(mission.when.replace('T', " ")))])];
    if !mission.until.is_empty() {
        facts.push(tf("s_al_beendet", &[("0", Arg::from(mission.until.replace('T', " ")))]));
    }
    if let Some((played, done)) = data.played.get(&mission.name) {
        facts.push(tf("s_al_oft", &[("0", Arg::from(*played)), ("1", Arg::from(*done))]));
    }
    ui.add(egui::Label::new(small(facts.join(" · "))).wrap());
    if !mission.source.is_empty() {
        ui.add(egui::Label::new(small(tf("s_al_quelle", &[("0", Arg::from(mission.source.as_str()))]))).wrap());
    }
    if !mission.blueprints.is_empty() {
        ui.add(egui::Label::new(RichText::new(blueprint_text(mission)).small().color(color(scheme.accent))).wrap());
    }
    ui.add_space(4.0);
    match (jump, ready) {
        (Some(jump), true) => {
            ui.horizontal_wrapped(|buttons| {
                if jump.is_some() {
                    let word = t("s_al_bp_zeigen");
                    let button = widgets::Button::new(&word, ButtonKind::Normal, scheme).icon("clipboard-list");
                    if buttons.add(button).on_hover_text(t("s_al_klick")).clicked() {
                        click = Some(RowClick::Jump);
                    }
                } else {
                    buttons.label(small(t("s_fo_lohnt_nichts")));
                }
                buttons.add_space(GAP);
                let word = t("s_al_ic");
                let button = widgets::Button::new(&word, ButtonKind::Normal, scheme).icon("bug");
                if buttons.add(button).on_hover_text(t("s_al_ic_tipp")).clicked() {
                    click = Some(RowClick::IssueCouncil);
                }
            });
        }
        // The language files are still being read.
        _ => {
            widgets::loading(ui, scheme, &t("s_uv_lese"));
        }
    }
    #[cfg(test)]
    if TOO_WIDE_ROW.get() {
        ui.add_sized([ui.available_width() + 40.0, 10.0], egui::Label::new("zu breit"));
    }
    click
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blueprint_list::{ListData, ListState};

    /// Lookups of the sample, ready before the first frame.
    fn ready(data: &LogData) -> &Lookups {
        data.start_lookups();
        data.lookups().expect("the sample builds them at once")
    }

    /// Rows open in turn, German and English, every scheme, size and width:
    /// nothing sticks out, neither out of the page nor inside the list.
    fn overflow(data: &mut LogData, catalog: &CatalogCache, broken: (bool, bool)) -> Vec<String> {
        use versekit_core::language::{Choice, set_thread_language};
        let mut findings = Vec::new();
        TOO_WIDE_ROW.set(broken.0);
        TOO_WIDE_HEAD.set(broken.1);
        for (language, scheme, points, width) in [Choice::De, Choice::En].into_iter().flat_map(|language| {
            versekit_core::theme::schemes().iter().flat_map(move |scheme| {
                (versekit_core::font_size::POINTS_RANGE.0..=versekit_core::font_size::POINTS_RANGE.1)
                    .flat_map(move |points| [420.0, 900.0].map(|width| (language, scheme, points, width)))
            })
        }) {
            set_thread_language(language);
            let ctx = egui::Context::default();
            crate::install(&ctx);
            crate::apply(&ctx, scheme);
            crate::apply_size(&ctx, points);
            let mut worst: f32 = 0.0;
            for open in [None, Some(0), Some(3), Some(5)] {
                let mut state = LogState { open, ..LogState::default() };
                for _ in 0..3 {
                    let input = egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, 2000.0))),
                        ..Default::default()
                    };
                    let mut output = ctx.run_ui(input, |ui| {
                        show(ui, scheme, data, catalog, &mut state);
                        // The outer area grows with what is drawn: measure against the window.
                        worst = worst.max(ui.min_rect().right() - ui.ctx().content_rect().right());
                    });
                    output.textures_delta.clear();
                    // Inside the list a row too wide would be clipped unseen.
                    worst = worst.max(state.row_overflow);
                }
            }
            if worst > 0.5 {
                findings.push(format!("{language:?} {} Größe {points} Breite {width}: {worst:.0} zu breit", scheme.id));
                // A counter-check only needs to see that something is found.
                if broken.0 || broken.1 {
                    break;
                }
            }
        }
        set_thread_language(Choice::Auto);
        TOO_WIDE_ROW.set(false);
        TOO_WIDE_HEAD.set(false);
        findings
    }

    #[test]
    fn folded_out_nothing_sticks_out() {
        let mut data = LogData::sample();
        ready(&data);
        let catalog = ListData::sample().catalog;
        let findings = overflow(&mut data, &catalog, (false, false));
        assert!(findings.is_empty(), "{}", findings.join("\n"));
    }

    /// Counter-checks: something too wide in an open row, or in the head, is found.
    #[test]
    fn the_edge_check_sees_what_is_too_wide() {
        let mut data = LogData::sample();
        ready(&data);
        let catalog = ListData::sample().catalog;
        assert!(!overflow(&mut data, &catalog, (true, false)).is_empty(), "row");
        assert!(!overflow(&mut data, &catalog, (false, true)).is_empty(), "head");
    }

    /// What `show` drew: counts over the search, rows of the chosen state,
    /// and running reads "still open" while the game is closed.
    #[test]
    fn toggles_count_over_the_search_and_running_reads_still_open() {
        // The words are compared with the language in force: another test that switches it must not run meanwhile.
        let _language = crate::language_lock();
        versekit_core::language::set_language(versekit_core::language::Choice::De);
        let mut data = LogData::sample();
        let ctx = egui::Context::default();
        crate::install(&ctx);
        let scheme = versekit_core::theme::current();
        let catalog = ListData::sample().catalog;
        let mut state = LogState { search: "  RETAKE ".into(), ..LogState::default() };
        let frame = |data: &mut LogData, state: &mut LogState| {
            let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
                show(ui, scheme, data, &catalog, state);
            });
            output.textures_delta.clear();
        };
        frame(&mut data, &mut state);
        let counts: Vec<usize> = state.drawn.0.iter().map(|(_, n)| *n).collect();
        assert_eq!(counts, [3, 0, 2, 1, 0, 0]);
        assert_eq!(state.drawn.1.len(), 3);
        assert_eq!(state.drawn.0[1].0, t("s_al_offen"));
        state.state = 2;
        frame(&mut data, &mut state);
        let names: Vec<&str> = state.drawn.1.iter().map(|&i| data.missions[i].name.as_str()).collect();
        assert_eq!(names, ["Retake the Outpost", "Retake the Outpost"]);
        assert!(state.drawn.1.iter().all(|&i| data.missions[i].state == log::COMPLETED));
        state.search.clear();
        state.state = 0;
        frame(&mut data, &mut state);
        assert_eq!(state.drawn.0[0].1, data.missions.len());
    }

    /// The page reads the game logs in the background and takes over what they
    /// say: an empty log fills, an unchanged reading changes nothing, and the
    /// sample never reads.
    #[test]
    fn the_logs_are_read_in_the_background_and_taken_over() {
        use versekit_core::legacy;
        use versekit_core::paths::{ENV_INSTALL_DIR, Host};
        use versekit_core::pypath::Flavor;

        let f = Flavor::native();
        let root = f.join(
            &std::env::temp_dir().to_string_lossy(),
            &format!("versekit-nachlese-{}", versekit_core::testdir::run_tag()),
        );
        let _ = std::fs::remove_dir_all(&root);
        let (home, game) = (f.join(&root, "home"), f.join_all(&root, &["StarCitizen", "LIVE"]));
        std::fs::create_dir_all(&home).expect("folder");
        std::fs::create_dir_all(f.join(&game, "logbackups")).expect("folder");
        std::fs::write(f.join(&game, "Game.log"), "").expect("file");
        let log = "<2026-10-01T10:00:00.000Z> OnClientSpawned\n\
                   <2026-10-01T10:00:05.000Z> <SHUDEvent_OnNotification> Added notification \"Auftrag angenommen: Hunt: \" [1] MissionId: [m1], ObjectiveId: []\n\
                   <2026-10-01T10:00:09.000Z> <SHUDEvent_OnNotification> Added notification \"Auftrag abgeschlossen: Hunt: \" [1] MissionId: [m1], ObjectiveId: []\n";
        std::fs::write(f.join_all(&game, &["logbackups", "a.log"]), log).expect("file");
        let host = Host::new(f, &[(legacy::ENV_HOME, home.as_str()), (ENV_INSTALL_DIR, game.as_str())], &root);
        let paths = Paths::new(host);
        let mut data = LogData::load(&paths);
        assert!(data.entries.is_empty());
        data.rescan(None);
        let started = std::time::Instant::now();
        while !data.poll() {
            assert!(started.elapsed().as_secs() < 30, "the reading did not finish");
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert_eq!(data.missions.len(), 1);
        assert_eq!((data.missions[0].name.as_str(), data.missions[0].state.as_str()), ("Hunt", log::COMPLETED));
        assert_eq!(data.played.get("Hunt"), Some(&(1, 1)));
        // Reading again finds the same: nothing changes, nothing to take over.
        data.rescan(None);
        while data.scanning() {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(!data.poll());
        // The reading obeys the write lock of the thread that started it.
        let other = log.replace("Hunt", "Rescue").replace("m1", "m2").replace("2026-10-01", "2026-10-02");
        std::fs::write(f.join_all(&game, &["logbackups", "b.log"]), other).expect("file");
        let before = std::fs::read(f.join(&home, "auftragslog.json")).expect("file");
        versekit_core::writeguard::deny(true);
        versekit_core::writeguard::allow_under(None);
        data.rescan(None);
        while data.scanning() {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        versekit_core::writeguard::deny(false);
        // Nothing of the new log was written, so nothing is there to take over.
        assert!(!data.poll());
        assert_eq!(std::fs::read(f.join(&home, "auftragslog.json")).expect("file"), before);
        // The sample reads nothing.
        let sample = LogData::sample();
        sample.rescan(None);
        assert!(!sample.scanning());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_jump_shows_exactly_the_blueprints_of_the_contract() {
        let data = LogData::sample();
        let list = ListData::sample();
        let lookups = ready(&data);
        let (title, keys) = jump_for(lookups, &list.catalog, "Retake the Outpost").expect("jump");
        assert_eq!(keys, lookups.blueprints.for_contract(&list.catalog, "Retake the Outpost"));
        let mut view = ListState::default();
        view.jump(ListJump::Contract(title, keys.clone()), "Aufträge & Protokoll: x".into());
        assert_eq!(crate::blueprint_list::shown_keys(&list, &view), keys);
        assert!(jump_for(lookups, &list.catalog, "Recover Vanduul Corpses").is_none());
        assert!(jump_for(lookups, &list.catalog, "  ").is_none());
    }

    /// The real data of a copy (`VERSEKIT_KOPIE`), read only: what the page
    /// shows. The language files only from `VERSEKIT_SPIELORDNER` (read
    /// only), without it none.
    #[test]
    #[ignore = "needs a copy of real data"]
    fn real_copy_counts() {
        let Ok(root) = std::env::var("VERSEKIT_KOPIE") else { return };
        versekit_core::writeguard::deny(true);
        versekit_core::writeguard::allow_under(None);
        let paths = Paths::for_copy(versekit_core::paths::Host::current(), &root).expect("copy");
        let languages: Vec<PathBuf> = match std::env::var("VERSEKIT_SPIELORDNER") {
            Ok(game) => {
                let base = std::path::Path::new(&game).join("data").join("Localization");
                let mut names: Vec<String> = std::fs::read_dir(&base)
                    .map(|r| r.filter_map(|e| e.ok()).map(|e| e.file_name().to_string_lossy().into_owned()).collect())
                    .unwrap_or_default();
                names.sort();
                names.into_iter().map(|n| base.join(n).join("global.ini")).filter(|p| p.is_file()).collect()
            }
            Err(_) => Vec::new(),
        };
        let data = LogData::load(&paths).with_languages(languages.clone());
        // The finished catalog, not the stand-in a window shows meanwhile.
        let list = ListData::load_with(&paths, versekit_gamedata::list::list_catalog(&paths).expect("catalog"));
        let mut per: Vec<(&str, usize)> = Vec::new();
        for s in STATES.iter().flatten() {
            per.push((s, data.missions.iter().filter(|m| m.state == *s).count()));
        }
        let names: Vec<&String> = {
            let mut seen = std::collections::HashSet::new();
            data.missions.iter().map(|m| &m.name).filter(|n| seen.insert(*n)).collect()
        };
        let started = std::time::Instant::now();
        data.start_lookups();
        let lookups = loop {
            if let Some(lookups) = data.lookups() {
                break lookups;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        };
        let built = started.elapsed();
        // The jump into the list must show exactly the contract's blueprints.
        let (mut exact, mut jumps) = (0, 0);
        for name in &names {
            let Some((title, keys)) = jump_for(lookups, &list.catalog, name) else { continue };
            jumps += 1;
            let mut view = ListState::default();
            view.jump(ListJump::Contract(title, keys.clone()), String::new());
            if crate::blueprint_list::shown_keys(&list, &view) == keys {
                exact += 1;
            }
        }
        let english = names.iter().filter(|n| lookups.search.search_text(n) != issue_council::clean(n)).count();
        println!("Sprachdateien {}", languages.len());
        println!("Aufträge {} · Namen {} · Zustände {per:?}", data.missions.len(), names.len());
        println!("mit Bauplan-Zeile {}", data.missions.iter().filter(|m| !m.blueprints.is_empty()).count());
        println!("Sprung zeigt genau die Baupläne des Auftrags: {exact} = {jumps}");
        println!("englisch gesucht {english} · Spiel läuft {} · Index gebaut in {built:?}", data.game_running());
        if let Some(first) = data.missions.first() {
            println!("erste Zeile {} {} {}", first.day(), first.state, first.name);
            println!("Issue Council {}", lookups.search.url_for(&first.name));
        }
        println!("nicht geschrieben (Sperre): {:?}", versekit_core::writeguard::take_denied());
        versekit_core::writeguard::deny(false);
        assert_eq!(exact, jumps, "Sprung zeigt andere Baupläne");
    }
}
