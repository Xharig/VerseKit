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

//! The page "Flugbuch", the sixth of the group "Statistik": the sessions, what happened in the chosen one in the
//! order it happened, a summary and the export of the lines the player chooses.
//!
//! ⚠ The flight log stays on this computer. The page reads `flugbuch.json` and the contract log, shows them and
//! sends nothing; the export is a file or the clipboard, handed on by the player. The shard stands only in the
//! hover of its line, and covered in streamer mode.

use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use egui::{FontSelection, RichText, Role, Sense, TextStyle, Ui, WidgetInfo, WidgetText, vec2};
use versekit_core::flugbuch::{self, Book, Data, Event, Item, Kind, Row, Session, Summary, Topic, Where};
use versekit_core::flugbuch_export::{self, Line, What};
use versekit_core::language::{self, Arg, Lang, t, tf};
use versekit_core::logsource::LogSource;
use versekit_core::play_stats;
use versekit_core::paths::{Host, Paths};
use versekit_core::theme::{IconTint, Scheme};
use versekit_core::{formats, playtime, shard, streamer, writeguard};

use crate::layout::{self, GAP};
use crate::widgets::{self, ButtonKind};
use crate::{color, file_dialog};

/// How often the open page loads the files again (seconds).
const REFRESH_SECONDS: f64 = 300.0;
/// Narrowest a tile gets before the tiles stack.
const TILE_MIN: f32 = 150.0;
/// How high the list of the sessions and the timeline stand at most; the rest scrolls.
const LIST_HEIGHT: f32 = 240.0;
const TIMELINE_HEIGHT: f32 = 360.0;
/// How many lines of the export the preview shows.
const PREVIEW_LINES: usize = 12;
/// The room between the time of day and the symbol of a line of the timeline.
const TIME_GAP: f32 = 8.0;
const ICON_SIZE: f32 = 16.0;

/// What the page shows: the book and what is worked out of it once.
pub struct View {
    book: Book,
    /// The sessions, the newest first.
    briefs: Vec<Brief>,
}

struct Brief {
    start: i64,
    summary: Summary,
    ship: Option<String>,
}

impl View {
    fn of(book: Book) -> View {
        let mut briefs: Vec<Brief> = book
            .sessions
            .iter()
            .map(|session| Brief {
                start: session.start,
                summary: book.summary(session),
                ship: book.main_ship(session).map(|class| book.ship_name(&class)),
            })
            .collect();
        briefs.sort_by_key(|brief| std::cmp::Reverse(brief.start));
        View { book, briefs }
    }

    fn session(&self, start: i64) -> Option<&Session> {
        self.book.sessions.iter().find(|s| s.start == start)
    }
}

#[derive(Default)]
struct Shared {
    running: bool,
    view: Option<Arc<View>>,
}

/// What a save of the export waits for.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Format {
    Text,
    Json,
}

/// The lines of the timeline as shown, for the session, the switches and the search as they were worked out.
struct Shown {
    key: (i64, u32, String, Lang, usize),
    entries: Vec<Entry>,
    /// The lines of the export of the same switches.
    lines: Vec<Line>,
    /// How tall each entry stands at the width it was last drawn at.
    metrics: Option<Metrics>,
}

/// The heights of the entries for one width and one size of text: `tops[i]` is where entry `i` starts and
/// `tops[len]` the height of all of them.
struct Metrics {
    key: (i32, i32, i32),
    tops: Vec<f32>,
}

enum Entry {
    /// The place the lines below happened at.
    Header(String),
    Line { time: String, icon: &'static str, text: String, tip: String },
}

/// The state of the page.
pub struct FlightState {
    /// Where the files are; none on the sample page.
    host: Option<Host>,
    view: Option<Arc<View>>,
    shared: Arc<Mutex<Shared>>,
    started: Option<f64>,
    last_frame: Option<u64>,
    /// The start of the chosen session.
    selected: Option<i64>,
    topics: HashSet<Topic>,
    search: String,
    shown: Option<Shown>,
    dialog: Option<(file_dialog::Pending, Format)>,
    notice: Option<String>,
    /// Everything the page said in the corner, for the tests.
    #[cfg(test)]
    said: Vec<String>,
}

impl FlightState {
    /// The state of the sample page: made-up lines, nothing is read.
    pub fn sample() -> FlightState {
        FlightState {
            host: None,
            view: Some(Arc::new(View::of(Book::of_sessions(vec![sample_session()])))),
            shared: Arc::default(),
            started: None,
            last_frame: None,
            selected: None,
            topics: Topic::ALL.into_iter().filter(|topic| topic.on_at_start()).collect(),
            search: String::new(),
            shown: None,
            dialog: None,
            notice: None,
            #[cfg(test)]
            said: Vec::new(),
        }
    }

    /// The state for the data folder of `paths`: the first look loads the files in the background.
    pub fn of(paths: &Paths) -> FlightState {
        FlightState { host: Some(paths.host.clone()), view: None, ..FlightState::sample() }
    }

    pub fn is_sample(&self) -> bool {
        self.host.is_none()
    }

    fn poll(&mut self) {
        let taken = self.shared.lock().unwrap_or_else(|e| e.into_inner()).view.take();
        if let Some(view) = taken {
            self.view = Some(view);
            self.shown = None;
        }
    }

    fn running(&self) -> bool {
        self.shared.lock().unwrap_or_else(|e| e.into_inner()).running
    }

    /// Loads the files in the background; nothing when a load is on.
    fn start(&mut self, ctx: &egui::Context, now: f64) {
        let Some(host) = self.host.clone() else { return };
        {
            let mut shared = self.shared.lock().unwrap_or_else(|e| e.into_inner());
            if shared.running {
                return;
            }
            shared.running = true;
        }
        self.started = Some(now);
        let (shared, ctx, lock) = (Arc::clone(&self.shared), ctx.clone(), writeguard::state());
        language::spawn_with_language(move || {
            writeguard::adopt(&lock);
            let paths = Paths::new(host);
            // The logs first: the last session still lies in the `Game.log` after the game is closed, and nothing
            // else fills the file while only this page is open.
            let source = LogSource::new(&paths, LogSource::sources_of(&paths));
            flugbuch::catch_up(&source, &play_stats::log_files(&paths));
            let book = Book::load(&paths);
            // The number of a shard is private in streamer mode: it is learned before anything is drawn.
            for event in book.sessions.iter().flat_map(|s| &s.events) {
                if let Some(id) = &event.data.shard {
                    shard::keep_private(&shard::Shard { id: id.clone() });
                }
            }
            let view = Arc::new(View::of(book));
            let mut shared = shared.lock().unwrap_or_else(|e| e.into_inner());
            shared.view = Some(view);
            shared.running = false;
            drop(shared);
            ctx.request_repaint();
        });
    }

    /// The window takes what the page said (to show in the corner).
    pub fn take_notice(&mut self) -> Option<String> {
        self.notice.take()
    }

    /// Waits for the load on, for tests.
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
        panic!("the load does not end");
    }

    fn topic_bits(&self) -> u32 {
        Topic::ALL.iter().enumerate().filter(|(_, topic)| self.topics.contains(topic)).fold(0, |bits, (i, _)| bits | 1 << i)
    }
}

/// A session of the sample page: a short flight with the made-up places and a contract.
fn sample_session() -> Session {
    let at = |seconds: i64| 1_790_848_800 + seconds;
    let event = |seconds: i64, kind: Kind, data: Data| Event { at: at(seconds), kind, data };
    let place = |number: &str| Data { place: Some(number.to_string()), ..Data::default() };
    Session {
        start: at(0),
        end: at(3600),
        account: String::new(),
        events: vec![
            event(0, Kind::Start, Data::default()),
            event(60, Kind::Place, place("1")),
            event(600, Kind::Place, place("0")),
            event(660, Kind::Target, Data { target: Some("rs_ext_nyx-castra_jp1".to_string()), ..Data::default() }),
            event(900, Kind::Jump, Data { target: Some("rs_ext_nyx-castra_jp1".to_string()), ship: Some("RSI_Polaris".to_string()), ..Data::default() }),
            event(3500, Kind::End, Data::default()),
        ],
    }
}

// ---------------------------------------------------------------------------
// Drawing
// ---------------------------------------------------------------------------

fn small(ui: &mut Ui, scheme: &Scheme, text: &str) {
    ui.add(egui::Label::new(RichText::new(text).small().color(color(scheme.sub))).wrap());
}

fn counted(n: usize, one: &str, many: &str) -> String {
    if n == 1 { t(one) } else { tf(many, &[("n", Arg::from(formats::count(language::current(), n as i64)))]) }
}

/// The line under the date of a session: the ship, then what was done.
fn brief_text(brief: &Brief) -> String {
    let s = &brief.summary;
    let mut parts: Vec<String> = Vec::new();
    parts.extend(brief.ship.clone());
    if s.jumps > 0 {
        parts.push(counted(s.jumps, "s_flb_z_sprung_eins", "s_flb_z_sprung"));
    }
    if s.contracts > 0 {
        parts.push(counted(s.contracts, "s_flb_z_auftrag_eins", "s_flb_z_auftrag"));
    }
    if s.deaths > 0 {
        parts.push(counted(s.deaths, "s_flb_z_tod_eins", "s_flb_z_tod"));
    }
    if parts.is_empty() { t("s_flb_z_nichts") } else { parts.join(" · ") }
}

/// A row of the list of sessions: the date and the length, under it what happened. The whole width, chosen = the
/// dark accent area with the bar at the left.
fn session_row(ui: &mut Ui, scheme: &Scheme, chosen: bool, title: &str, below: &str, name: &str) -> egui::Response {
    let inner = (ui.available_width() - 20.0).max(1.0);
    let head = WidgetText::from(RichText::new(title).strong().color(color(scheme.fg))).into_galley(
        ui,
        Some(egui::TextWrapMode::Wrap),
        inner,
        FontSelection::Style(TextStyle::Body),
    );
    let foot = WidgetText::from(RichText::new(below).small().color(color(scheme.sub))).into_galley(
        ui,
        Some(egui::TextWrapMode::Wrap),
        inner,
        FontSelection::Style(TextStyle::Small),
    );
    let height = head.size().y + foot.size().y + 12.0;
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::click());
    let enabled = ui.is_enabled();
    response.widget_info(|| WidgetInfo::selected(Role::ListBoxOption, enabled, chosen, name));
    if ui.is_rect_visible(rect) {
        let radius = crate::control_radius(scheme);
        if chosen {
            ui.painter().rect_filled(rect, radius, color(scheme.accent_dark));
            let bar = egui::Rect::from_min_size(rect.min, vec2(widgets::LIST_BAR_WIDTH, rect.height()));
            ui.painter().rect_filled(bar, egui::CornerRadius::ZERO, color(scheme.accent));
        } else if response.hovered() {
            ui.painter().rect_filled(rect, radius, color(scheme.hover));
        }
        let left = rect.min.x + 10.0;
        let top = rect.min.y + 5.0;
        let below_at = top + head.size().y + 2.0;
        ui.painter().galley(egui::pos2(left, top), head, color(scheme.fg));
        ui.painter().galley(egui::pos2(left, below_at), foot, color(scheme.sub));
    }
    widgets::focus_ring(ui, &response, scheme, crate::control_radius(scheme));
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Figures without a frame, each of equal width.
fn tiles(ui: &mut Ui, scheme: &Scheme, items: &[(String, String)], columns: usize) {
    for (row, chunk) in items.chunks(columns).enumerate() {
        if row > 0 {
            ui.add_space(GAP);
        }
        layout::columns_or_stack(ui, TILE_MIN, columns, |ui, i| {
            let Some((title, value)) = chunk.get(i) else { return };
            ui.vertical(|ui| {
                ui.set_min_width(ui.available_width());
                small(ui, scheme, title);
                let size = TextStyle::Heading.resolve(ui.style()).size;
                ui.add(
                    egui::Label::new(RichText::new(value).font(crate::fonts::bold(size)).color(color(scheme.accent)))
                        .wrap(),
                );
            });
        });
    }
}

fn topic_name(topic: Topic) -> String {
    t(match topic {
        Topic::Session => "s_flb_t_session",
        Topic::Place => "s_flb_t_place",
        Topic::Target => "s_flb_t_target",
        Topic::Jump => "s_flb_t_jump",
        Topic::Contract => "s_flb_t_contract",
        Topic::Find => "s_flb_t_find",
        Topic::Death => "s_flb_t_death",
        Topic::Injury => "s_flb_t_injury",
        Topic::Connection => "s_flb_t_connection",
        Topic::Shard => "s_flb_t_shard",
        Topic::Ship => "s_flb_t_ship",
        Topic::Loading => "s_flb_t_loading",
    })
}

/// The symbol of a topic: one meaning, one symbol.
fn topic_icon(topic: Topic) -> &'static str {
    match topic {
        Topic::Session => "circle-play",
        Topic::Place => "map-pin",
        Topic::Target => "route",
        Topic::Jump => "orbit",
        Topic::Contract => "crosshair",
        Topic::Find => "clipboard-list",
        Topic::Death => "circle-x",
        Topic::Injury => "stethoscope",
        Topic::Connection => "circle-alert",
        Topic::Shard => "server",
        Topic::Ship => "plane-takeoff",
        Topic::Loading => "timer",
    }
}

/// The symbol of a line: the one of its topic, but the end and the crash of a session have their own.
fn row_icon(row: &Row) -> &'static str {
    match &row.item {
        Item::Event(Event { kind: Kind::End, .. }) => "door-open",
        Item::Event(Event { kind: Kind::Crash, .. }) => "triangle-alert",
        _ => topic_icon(row.topic()),
    }
}

fn clock(at: i64) -> String {
    formats::local_time(at).map(|time| formats::strftime_shown("%H:%M:%S", &time)).unwrap_or_default()
}

/// The place of a header: the readable name, "on the move", or "unknown place".
fn place_text(place: &Where) -> Option<String> {
    match place {
        Where::Unknown => None,
        Where::Underway => Some(t("s_uv_unterwegs")),
        Where::At(Some(name)) => Some(name.clone()),
        Where::At(None) => Some(t("s_uv_ort_unbekannt")),
    }
}

/// The sentence of a row, and what stands in its hover.
fn row_text(book: &Book, row: &Row, lang: Lang) -> (String, String) {
    if let Some(line) = flugbuch_export::line_of(book, row, lang) {
        let mut text = flugbuch_export::sentence(&line, lang);
        if let Item::Event(Event { kind: Kind::Target, data, .. }) = &row.item
            && let Some(from) = &data.from
        {
            let to = line.place.clone().unwrap_or_else(|| language::t_in(lang, "s_uv_ort_unbekannt"));
            text = language::tf_in(lang, "s_flb_e_ziel_ab", &[("0", Arg::from(to)), ("1", Arg::from(from.clone()))]);
        }
        let tip = match (&row.item, line.what) {
            (Item::Event(Event { data: Data { shard: Some(id), .. }, .. }), What::ShardChanged) => {
                streamer::scrub(&language::tf_in(lang, "s_flb_e_shard_tip", &[("shard", Arg::from(id.clone()))]))
            }
            _ => text.clone(),
        };
        return (text, tip);
    }
    let text = match &row.item {
        Item::Event(Event { kind: Kind::Ship, data, .. }) => language::tf_in(
            lang,
            "s_flb_e_schiff",
            &[("0", Arg::from(data.ship.as_deref().map(|class| book.ship_name(class)).unwrap_or_default()))],
        ),
        Item::Event(Event { kind: Kind::Loading, data, .. }) => language::tf_in(
            lang,
            "s_flb_e_lade",
            &[
                ("0", Arg::from(data.name.clone().unwrap_or_default())),
                ("1", Arg::from(data.seconds.unwrap_or(0))),
            ],
        ),
        _ => String::new(),
    };
    (text.clone(), text)
}

/// The lines of the timeline for what is chosen.
fn shown_of(view: &View, session: &Session, topics: &HashSet<Topic>, search: &str, lang: Lang) -> (Vec<Entry>, Vec<Line>) {
    let book = &view.book;
    let needle = search.trim().to_lowercase();
    let mut entries = Vec::new();
    let mut lines = Vec::new();
    let mut header: Option<String> = None;
    for row in book.rows(session).iter().filter(|row| topics.contains(&row.topic())) {
        let (text, tip) = row_text(book, row, lang);
        let place = place_text(&row.place);
        if !needle.is_empty()
            && !text.to_lowercase().contains(&needle)
            && !place.as_deref().is_some_and(|p| p.to_lowercase().contains(&needle))
        {
            continue;
        }
        if place != header {
            if let Some(name) = &place {
                entries.push(Entry::Header(name.clone()));
            }
            header = place;
        }
        entries.push(Entry::Line { time: clock(row.at), icon: row_icon(row), text, tip });
        if row.topic().exportable()
            && let Some(line) = flugbuch_export::line_of(book, row, lang)
        {
            lines.push(line);
        }
    }
    (entries, lines)
}

/// Draws the page. Returns what it wants said in the corner of the window.
pub fn show(ui: &mut Ui, scheme: &Scheme, state: &mut FlightState) -> Option<String> {
    let said = draw(ui, scheme, state);
    #[cfg(test)]
    state.said.extend(said.clone());
    said
}

fn draw(ui: &mut Ui, scheme: &Scheme, state: &mut FlightState) -> Option<String> {
    state.poll();
    state.finish_save();
    let now = ui.input(|i| i.time);
    // Entering the page loads the files; staying on it, every few minutes.
    let frame = ui.ctx().cumulative_frame_nr();
    let entered = state.last_frame.is_none_or(|last| frame > last + 1);
    state.last_frame = Some(frame);
    if !state.is_sample() && (entered || state.started.is_none_or(|at| now - at >= REFRESH_SECONDS)) {
        state.start(ui.ctx(), now);
    }
    if state.running() {
        ui.ctx().request_repaint_after(Duration::from_millis(250));
    } else if !state.is_sample() {
        ui.ctx().request_repaint_after(Duration::from_secs_f64(REFRESH_SECONDS));
    }
    small(ui, scheme, &t("s_flb_lead"));
    ui.add_space(GAP);
    let Some(view) = state.view.clone() else {
        widgets::loading(ui, scheme, &t("s_sa_laedt"));
        return state.take_notice();
    };
    let lang = language::current();
    widgets::card(ui, scheme, |ui| {
        widgets::card_title(ui, scheme, &t("s_flb_sitzungen"));
        ui.add_space(4.0);
        if view.briefs.is_empty() {
            widgets::empty_state(ui, scheme, "plane-landing", &t("s_flb_leer"));
            return;
        }
        sessions_list(ui, scheme, state, &view, lang);
    });
    let chosen = state.selected.filter(|start| view.session(*start).is_some()).or_else(|| view.briefs.first().map(|b| b.start));
    let Some(start) = chosen else { return state.take_notice() };
    state.selected = Some(start);
    let (Some(session), Some(brief)) = (view.session(start), view.briefs.iter().find(|b| b.start == start)) else {
        return state.take_notice();
    };
    ui.add_space(GAP);
    widgets::card(ui, scheme, |ui| {
        widgets::card_title(ui, scheme, &t("s_flb_zusammenfassung"));
        ui.add_space(4.0);
        summary(ui, scheme, &brief.summary, lang);
    });
    ui.add_space(GAP);
    widgets::card(ui, scheme, |ui| timeline(ui, scheme, state, &view, session, lang));
    ui.add_space(GAP);
    widgets::card(ui, scheme, |ui| export(ui, scheme, state, session, lang));
    state.take_notice()
}

fn sessions_list(ui: &mut Ui, scheme: &Scheme, state: &mut FlightState, view: &View, lang: Lang) {
    let mut picked = None;
    let area = egui::ScrollArea::vertical()
        .id_salt("flight-sessions")
        .auto_shrink([false, true])
        .max_height(LIST_HEIGHT)
        .min_scrolled_height(LIST_HEIGHT);
    widgets::scroll_area(ui, scheme, area, |ui| {
        for brief in &view.briefs {
            let when = formats::date_time(lang, brief.start as f64);
            let end = view.session(brief.start).map_or(brief.start, |s| s.end);
            let title = format!("{when} · {}", playtime::as_text(end - brief.start));
            let below = brief_text(brief);
            let name = tf("s_flb_z_name", &[("when", Arg::from(when)), ("summary", Arg::from(below.clone()))]);
            let chosen = state.selected == Some(brief.start);
            let row = session_row(ui, scheme, chosen, &title, &below, &name).on_hover_text(t("s_flb_z_tip"));
            if row.clicked() {
                picked = Some(brief.start);
            }
        }
    });
    if let Some(start) = picked {
        state.selected = Some(start);
    }
}

fn summary(ui: &mut Ui, scheme: &Scheme, s: &Summary, lang: Lang) {
    let count = |n: usize| formats::count(lang, n as i64);
    let items = [
        (t("s_flb_k_dauer"), playtime::as_text(s.duration)),
        (t("s_flb_k_spruenge"), count(s.jumps)),
        (t("s_flb_k_auftraege"), count(s.contracts)),
        (t("s_flb_k_funde"), count(s.finds)),
        (t("s_flb_k_tode"), count(s.deaths)),
        (t("s_flb_k_server"), count(s.server_changes)),
        (t("s_flb_k_orte"), count(s.places)),
    ];
    let columns = (ui.available_width() / TILE_MIN).floor().clamp(1.0, 4.0) as usize;
    tiles(ui, scheme, &items, columns);
}

fn timeline(ui: &mut Ui, scheme: &Scheme, state: &mut FlightState, view: &View, session: &Session, lang: Lang) {
    widgets::card_title(ui, scheme, &t("s_flb_zeitstrahl"));
    ui.add_space(4.0);
    // The switches, one per kind of event, side by side where the page is wide.
    let columns = ((ui.available_width() / 260.0).floor() as usize).clamp(1, 3);
    let per = Topic::ALL.len().div_ceil(columns);
    layout::columns_or_stack_loose(ui, 240.0, columns, |ui, column| {
        for topic in Topic::ALL.iter().skip(column * per).take(per) {
            let mut on = state.topics.contains(topic);
            let name = topic_name(*topic);
            let help = tf("s_flb_t_tip", &[("topic", Arg::from(name.clone()))]);
            if widgets::switch_row_help(ui, scheme, &name, &help, &mut on).changed() {
                if on {
                    state.topics.insert(*topic);
                } else {
                    state.topics.remove(topic);
                }
            }
        }
    });
    ui.add_space(GAP);
    let width = ui.available_width().min(420.0);
    widgets::search_field(ui, &mut state.search, &t("s_flb_suche"), scheme, width).on_hover_text(t("s_flb_suche_tip"));
    ui.add_space(GAP);
    let loaded = state.view.as_ref().map_or(0, |view| Arc::as_ptr(view) as usize);
    let key = (session.start, state.topic_bits(), state.search.clone(), lang, loaded);
    if state.shown.as_ref().is_none_or(|shown| shown.key != key) {
        let (entries, lines) = shown_of(view, session, &state.topics, &state.search, lang);
        state.shown = Some(Shown { key, entries, lines, metrics: None });
    }
    let Some(shown) = &mut state.shown else { return };
    if shown.entries.is_empty() {
        small(ui, scheme, &t("s_flb_kein_treffer"));
        return;
    }
    let area = egui::ScrollArea::vertical()
        .id_salt("flight-timeline")
        .auto_shrink([false, true])
        .max_height(TIMELINE_HEIGHT)
        .min_scrolled_height(TIMELINE_HEIGHT);
    widgets::scroll_area(ui, scheme, area, |ui| {
        // Only the lines in view are drawn. A long line wraps, so each entry is as tall as its text needs at this
        // width; the heights are worked out once for a width and a size of text.
        ui.spacing_mut().item_spacing.y = 0.0;
        let width = ui.available_width();
        // The width of the time follows the size of the text: the widest clock reading.
        let time_width = widgets::one_line_width(ui, RichText::new("00:00:00").small(), TextStyle::Small) + TIME_GAP;
        let key = (
            width.round() as i32,
            (ui.ctx().pixels_per_point() * 100.0).round() as i32,
            (ui.text_style_height(&TextStyle::Body) * 10.0).round() as i32,
        );
        if shown.metrics.as_ref().is_none_or(|metrics| metrics.key != key) {
            let mut tops = Vec::with_capacity(shown.entries.len() + 1);
            let mut top = 0.0;
            for entry in &shown.entries {
                tops.push(top);
                top += entry_height(ui, entry, width, time_width);
            }
            tops.push(top);
            shown.metrics = Some(Metrics { key, tops });
        }
        let (Some(metrics), entries) = (&shown.metrics, &shown.entries) else { return };
        let tops = &metrics.tops;
        let clip = ui.clip_rect();
        let origin = ui.cursor().min.y;
        let first = tops.partition_point(|top| *top <= clip.min.y - origin).saturating_sub(1).min(entries.len());
        let last = (tops.partition_point(|top| *top < clip.max.y - origin) + 1).min(entries.len());
        ui.add_space(tops[first]);
        for (entry, at) in entries[first..last].iter().zip(first..) {
            entry_row(ui, scheme, entry, tops[at + 1] - tops[at], time_width);
        }
        ui.add_space(tops[entries.len()] - tops[last]);
    });
}

/// The room beside the text of an entry: the time and the symbol of a line, the symbol of a header.
fn text_offset(entry: &Entry, time_width: f32) -> f32 {
    match entry {
        Entry::Header(_) => ICON_SIZE + TIME_GAP,
        Entry::Line { .. } => time_width + ICON_SIZE + TIME_GAP,
    }
}

/// How tall an entry stands when its text wraps at the room left beside the time and the symbol.
fn entry_height(ui: &Ui, entry: &Entry, width: f32, time_width: f32) -> f32 {
    let room = (width - text_offset(entry, time_width)).max(1.0);
    let text = match entry {
        Entry::Header(name) => RichText::new(name).strong(),
        Entry::Line { text, .. } => RichText::new(text),
    };
    widgets::wrapped_height(ui, text, room, TextStyle::Body).max(ICON_SIZE) + 8.0
}

fn entry_row(ui: &mut Ui, scheme: &Scheme, entry: &Entry, height: f32, time_width: f32) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let line = ui.text_style_height(&TextStyle::Body).max(ICON_SIZE);
    let top = rect.min.y + 4.0;
    let at = |x: f32, width: f32| egui::Rect::from_min_size(egui::pos2(x, top), vec2(width, line));
    let icon_at = |x: f32| egui::Rect::from_min_size(egui::pos2(x, top + (line - ICON_SIZE) / 2.0), vec2(ICON_SIZE, ICON_SIZE));
    let text_x = rect.min.x + text_offset(entry, time_width);
    let text_room = egui::Rect::from_min_max(egui::pos2(text_x, top), egui::pos2(rect.max.x.max(text_x + 1.0), rect.max.y));
    let beside = egui::Layout::top_down(egui::Align::Min);
    match entry {
        Entry::Header(name) => {
            if let Some(image) = crate::icons::themed("map-pin", scheme, IconTint::Highlight, ICON_SIZE) {
                ui.put(icon_at(rect.min.x), image);
            }
            ui.scope_builder(egui::UiBuilder::new().max_rect(text_room).layout(beside), |ui| {
                let label = egui::Label::new(RichText::new(name).strong().color(color(scheme.accent))).wrap();
                ui.add(label.sense(Sense::hover())).on_hover_text(name);
            });
        }
        Entry::Line { time, icon, text, tip } => {
            let clock = egui::Label::new(RichText::new(time).small().color(color(scheme.sub)));
            ui.put(at(rect.min.x, time_width - TIME_GAP), clock);
            if let Some(image) = crate::icons::themed(icon, scheme, IconTint::Normal, ICON_SIZE) {
                ui.put(icon_at(rect.min.x + time_width), image);
            }
            ui.scope_builder(egui::UiBuilder::new().max_rect(text_room).layout(beside), |ui| {
                ui.add(egui::Label::new(text.as_str()).wrap().sense(Sense::hover())).on_hover_text(tip);
            });
        }
    }
}

fn export(ui: &mut Ui, scheme: &Scheme, state: &mut FlightState, session: &Session, lang: Lang) {
    widgets::card_title(ui, scheme, &t("s_flb_export"));
    ui.add_space(4.0);
    // What the file holds stands in the hover of the line, not in a block of text.
    let hint = egui::Label::new(RichText::new(t("s_flb_export_hinweis")).small().color(color(scheme.sub))).wrap();
    ui.add(hint.sense(Sense::hover())).on_hover_text(t("s_flb_export_inhalt"));
    ui.add_space(GAP);
    let lines: Vec<Line> = state.shown.as_ref().map(|shown| shown.lines.clone()).unwrap_or_default();
    if lines.is_empty() {
        small(ui, scheme, &t("s_flb_export_leer"));
        return;
    }
    let text = flugbuch_export::render_text(&lines, session, lang);
    ui.label(RichText::new(t("s_flb_export_vorschau")).strong());
    // The head line and the empty line between it and the events do not count as lines of events.
    let preview: Vec<&str> = text.lines().take(PREVIEW_LINES + 2).collect();
    ui.add(egui::Label::new(RichText::new(preview.join("\n")).monospace().small().color(color(scheme.sub))).wrap());
    let shown = lines.len().min(PREVIEW_LINES);
    small(
        ui,
        scheme,
        &tf("s_flb_export_zeilen", &[("shown", Arg::from(shown as i64)), ("total", Arg::from(lines.len() as i64))]),
    );
    ui.add_space(GAP);
    let mut pressed = None;
    let (copy_text, text_text, json_text) = (t("s_flb_export_kopieren"), t("s_flb_export_text"), t("s_flb_export_json"));
    ui.horizontal_wrapped(|row| {
        let copy = widgets::Button::new(&copy_text, ButtonKind::Normal, scheme).icon("copy");
        if row.add(copy).on_hover_text(t("s_flb_export_kopieren_tip")).clicked() {
            pressed = Some(None);
        }
        let as_text = widgets::Button::new(&text_text, ButtonKind::Normal, scheme).icon("save");
        if row.add(as_text).on_hover_text(t("s_flb_export_text_tip")).clicked() {
            pressed = Some(Some(Format::Text));
        }
        let as_json = widgets::Button::new(&json_text, ButtonKind::Normal, scheme).icon("save");
        if row.add(as_json).on_hover_text(t("s_flb_export_json_tip")).clicked() {
            pressed = Some(Some(Format::Json));
        }
    });
    match pressed {
        Some(None) => {
            ui.ctx().copy_text(text);
            state.notice = Some(t("s_flb_export_kopiert"));
        }
        Some(Some(format)) => state.ask_save(ui.ctx(), session, format),
        None => {}
    }
}

impl FlightState {
    fn ask_save(&mut self, ctx: &egui::Context, session: &Session, format: Format) {
        let day = formats::local_time(session.start).map_or_else(String::new, |time| formats::strftime("%Y-%m-%d", &time));
        let (extension, label) = match format {
            Format::Text => ("txt", "Text"),
            Format::Json => ("json", "JSON"),
        };
        let name = format!("versekit-flugbuch-{day}.{extension}");
        // The sample page writes nothing and opens no dialog: it only says so.
        let Some(host) = &self.host else {
            self.notice = Some(tf("s_flb_export_gespeichert", &[("name", Arg::from(name))]));
            return;
        };
        let paths = Paths::new(host.clone());
        let start = versekit_core::export::FileExports {
            paths: &paths,
            version: env!("CARGO_PKG_VERSION"),
            clock: versekit_core::export::now_unix,
        }
        .folder();
        let filters: file_dialog::Filters = &[(label, &[extension])];
        let pending = file_dialog::save_file(ctx, &t("s_flb_export"), filters, Some(&start), &name);
        self.dialog = Some((pending, format));
    }

    /// The answer of the save dialog: writes the file.
    fn finish_save(&mut self) {
        let Some(answer) = self.dialog.as_ref().and_then(|(pending, _)| pending.take()) else { return };
        let Some((_, format)) = self.dialog.take() else { return };
        // Cancelled: nothing is said.
        let Some(mut target) = answer.filter(|p| !p.is_empty()) else { return };
        let extension = if format == Format::Text { ".txt" } else { ".json" };
        if !target.to_lowercase().ends_with(extension) {
            target.push_str(extension);
        }
        let (Some(shown), Some(view)) = (&self.shown, &self.view) else { return };
        let Some(session) = view.session(shown.key.0) else { return };
        let lang = language::current();
        let body = match format {
            Format::Text => flugbuch_export::render_text(&shown.lines, session, lang),
            Format::Json => flugbuch_export::render_json(&shown.lines, session),
        };
        self.notice = Some(match std::fs::write(&target, body) {
            Ok(()) => {
                let name = std::path::Path::new(&target).file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned());
                tf("s_flb_export_gespeichert", &[("name", Arg::from(name))])
            }
            Err(_) => t("s_flb_export_fehler"),
        });
    }
}

#[cfg(test)]
mod tests;
