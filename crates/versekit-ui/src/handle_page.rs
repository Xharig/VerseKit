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

//! The page "Handle-Suche" of the main window, with two tabs on one state:
//!
//! - "Suche": the field and the card of the search at the top, below them the handles searched lately, the
//!   newest first. The members of an organization opened on the card stand in the card, as in the info bar.
//! - "Beziehungen": the handles and organizations marked, with the export and the import of the list. The
//!   handles searched lately without a mark stand there too, under the filter "unknown".
//!
//! Both lists share one build: it sorts by stance, kind, name or the time searched and filters by stance, kind
//! and a text. A click on a handle searches it, a click on an organization opens its card, both on the tab of
//! the search; the marks of a row set or take away its stance. The bin of a row deletes its mark; a notice puts
//! it back for a few seconds.
//!
//! The overlay runs in a process of its own and keeps the relations and settings in memory: after every change
//! here it is told to read them anew. A change the overlay wrote shows here within a second, from the times the
//! files were written.

use std::time::SystemTime;

use egui::{Align, Layout, RichText, Role, Sense, Ui, WidgetInfo};
use versekit_core::handle_query::members::{Member, Roster};
use versekit_core::handle_query::relations::{self, Kind, Relation, Relations, Stance};
use versekit_core::handle_query::{Card, Found, Membership, Org, Orgs, Profile, cache};
use versekit_core::language::{self, Arg, t, tf};
use versekit_core::paths::Paths;
use versekit_core::theme::{Color, Scheme};

use crate::color;
use crate::handle_search::{self, HandleSearch};
use crate::layout::GAP;
use crate::widgets::{self, ButtonKind};

/// How many rows the list shows at first, and how many more each click on "show more" adds.
const PAGE_ROWS: usize = 200;
/// The widths of the columns beside the name.
const STANCE_WIDTH: f32 = 110.0;
const KIND_WIDTH: f32 = 70.0;
const SEARCHED_WIDTH: f32 = 110.0;
/// The narrowest the name column gets.
const NAME_MIN_WIDTH: f32 = 60.0;
/// The size of the marks in a row, as on the card.
const MARK_SIZE: f32 = 16.0;
/// The icon of a handle searched without a mark.
const UNKNOWN_ICON: &str = "circle-question-mark";
/// How often the files are looked at for a change from the overlay, in seconds.
const LOOK_SECONDS: f64 = 1.0;

/// What the list sorts by.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Column {
    Stance,
    Kind,
    Name,
    Searched,
}

/// The choices of the stance filter: every row marked, then one stance each, `Some(None)` for unknown.
const STANCE_FILTERS: [Option<Option<Stance>>; 5] =
    [None, Some(Some(Stance::Friendly)), Some(Some(Stance::Neutral)), Some(None), Some(Some(Stance::Hostile))];
/// The choices of the kind filter.
const KIND_FILTERS: [Option<Kind>; 3] = [None, Some(Kind::Handle), Some(Kind::Org)];

/// One row of the list: a handle or the SID of an organization, its stance (`None` for a handle searched
/// without a mark) and when it was searched last, as far as the handles kept say.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub kind: Kind,
    pub name: String,
    pub stance: Option<Stance>,
    pub searched: Option<f64>,
}

/// How the list is sorted and filtered.
#[derive(Clone, Debug, PartialEq)]
pub struct View {
    pub sort: Column,
    pub ascending: bool,
    /// An index into the stance filter: 0 every row marked, 1 friendly, 2 neutral, 3 unknown, 4 hostile.
    pub stance: usize,
    /// An index into the kind filter: 0 all, 1 handles, 2 organizations.
    pub kind: usize,
    /// A part of the name, capitals and small letters alike.
    pub text: String,
    /// The list of the handles searched lately: every row searched, whatever its stance; the stance filter counts
    /// not.
    pub recent: bool,
}

impl Default for View {
    fn default() -> View {
        View { sort: Column::Stance, ascending: true, stance: 0, kind: 0, text: String::new(), recent: false }
    }
}

impl View {
    /// The view of the handles searched lately: the newest first.
    pub fn recent() -> View {
        View { sort: Column::Searched, recent: true, ..View::default() }
    }
}

/// The rows of `relations` and of the handles searched lately (`recent`, handle and time): a handle searched
/// that is marked gets the time on its row, one without a mark gets a row of its own.
pub fn rows(relations: &Relations, recent: &[(String, f64)]) -> Vec<Row> {
    let mut rows: Vec<Row> = relations
        .list()
        .iter()
        .map(|r| Row { kind: r.kind, name: r.name.clone(), stance: Some(r.stance), searched: None })
        .collect();
    let mut handles: std::collections::HashMap<String, usize> = rows
        .iter()
        .enumerate()
        .filter(|(_, row)| row.kind == Kind::Handle)
        .map(|(i, row)| (row.name.to_lowercase(), i))
        .collect();
    for (handle, at) in recent {
        match handles.get(&handle.to_lowercase()) {
            Some(&i) => rows[i].searched = Some(rows[i].searched.map_or(*at, |s| s.max(*at))),
            None => {
                handles.insert(handle.to_lowercase(), rows.len());
                rows.push(Row { kind: Kind::Handle, name: handle.clone(), stance: None, searched: Some(*at) });
            }
        }
    }
    rows
}

/// The place of a stance in the list: friendly, neutral, unknown, hostile.
fn stance_rank(stance: Option<Stance>) -> u8 {
    match stance {
        Some(Stance::Friendly) => 0,
        Some(Stance::Neutral) => 1,
        None => 2,
        Some(Stance::Hostile) => 3,
    }
}

fn kind_rank(kind: Kind) -> u8 {
    match kind {
        Kind::Handle => 0,
        Kind::Org => 1,
    }
}

/// The rows `view` lets through, in its order. Rows that are equal in the column sorted stand by name.
pub fn arrange(rows: &[Row], view: &View) -> Vec<Row> {
    let stance = STANCE_FILTERS.get(view.stance).copied().flatten();
    let kind = KIND_FILTERS.get(view.kind).copied().flatten();
    let text = view.text.trim().to_lowercase();
    let fits_stance = |row: &Row| match (view.recent, stance) {
        (true, _) => row.searched.is_some(),
        (false, None) => row.stance.is_some(),
        (false, Some(s)) => row.stance == s,
    };
    let mut shown: Vec<(String, &Row)> = rows
        .iter()
        .filter(|row| fits_stance(row))
        .filter(|row| kind.is_none_or(|k| row.kind == k))
        .map(|row| (row.name.to_lowercase(), row))
        .filter(|(name, _)| text.is_empty() || name.contains(&text))
        .collect();
    shown.sort_by(|(a_name, a), (b_name, b)| {
        let by_name = a_name.cmp(b_name);
        let order = match view.sort {
            Column::Stance => stance_rank(a.stance).cmp(&stance_rank(b.stance)),
            Column::Kind => kind_rank(a.kind).cmp(&kind_rank(b.kind)),
            Column::Name => by_name.then(kind_rank(a.kind).cmp(&kind_rank(b.kind))),
            // The newest first; a row never searched last.
            Column::Searched => {
                b.searched.unwrap_or(f64::NEG_INFINITY).total_cmp(&a.searched.unwrap_or(f64::NEG_INFINITY))
            }
        };
        let order = if view.ascending { order } else { order.reverse() };
        order.then(by_name)
    });
    shown.into_iter().map(|(_, row)| row.clone()).collect()
}

/// When the files the page follows were written last and how long they are: relations, settings, handles kept.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Stamps([Option<(SystemTime, u64)>; 3]);

impl Stamps {
    fn of(paths: &Paths) -> Stamps {
        let written = |path: String| std::fs::metadata(path).and_then(|m| Ok((m.modified()?, m.len()))).ok();
        Stamps([
            written(paths.app_file(relations::FILE)),
            written(paths.settings_file()),
            written(paths.app_file(cache::CACHE_FILE)),
        ])
    }
}

/// The handles kept within the days set, the newest first, as the site writes them.
fn recent_of(paths: &Paths) -> Vec<(String, f64)> {
    let days = cache::Settings::read(paths).days;
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0.0, |d| d.as_secs_f64());
    cache::recent(paths, now, days).into_iter().rev().map(|(_, card, at)| (card.profile.handle, at)).collect()
}

/// Tells the overlay to read the settings and relations anew.
#[cfg(not(test))]
fn tell_overlay() {
    use versekit_core::window_signal::{self, Role, Signal};
    window_signal::send_to(Role::Overlay, Signal::Settings);
}

/// The tests of this crate never reach a window of another process.
#[cfg(test)]
fn tell_overlay() {}

/// The made-up card of the preview and the members of its organization, one of them hidden.
fn sample_card(now: f64) -> (Found, Roster) {
    let org = Org {
        sid: "NOVA".into(),
        name: "Nova Wing".into(),
        rank: "Pilot".into(),
        stars: 3,
        members: Some(48),
        commitment: "Regular".into(),
        activity: "Security".into(),
        ..Org::default()
    };
    let profile = Profile {
        handle: "NightOwl_77".into(),
        name: "Night Owl".into(),
        enlisted: "Mar 3, 2019".into(),
        ..Profile::default()
    };
    let card =
        Card { profile, orgs: Orgs { main: Some(Membership::Shown(org)), affiliations: vec![Membership::Hidden] } };
    let found = Found { card, fetched: now, live: true, failed: None };
    let member = |handle: &str, rank: &str, stars: u8| Member { handle: handle.into(), rank: rank.into(), stars };
    let members = vec![
        member("Starlight_Nova", "Founder", 5),
        member("NightOwl_77", "Pilot", 3),
        member("QuietComet", "Officer", 4),
        member("", "Pilot", 2),
        member("Vex", "Recruit", 0),
        member("Drifter_9", "Pilot", 2),
        member("Zyx_Qorvath", "Commander", 1),
    ];
    let roster = Roster { sid: "NOVA".into(), members, total: Some(48), pages: 1, end: false, fetched: now };
    (found, roster)
}

/// Which of the two lists: the relations (tab "Beziehungen") or the handles searched lately (tab "Suche").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Which {
    Relations,
    Recent,
}

/// The tabs of the page.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tab {
    #[default]
    Search,
    Relations,
}

impl Tab {
    const ALL: [Tab; 2] = [Tab::Search, Tab::Relations];

    fn index(self) -> usize {
        Tab::ALL.iter().position(|tab| *tab == self).unwrap_or(0)
    }

    fn label(self) -> String {
        t(match self {
            Tab::Search => "s_hqt_reiter_suche",
            Tab::Relations => "s_hqt_reiter_beziehungen",
        })
    }
}

/// One list as its page shows it.
pub struct ListState {
    pub view: View,
    /// The filters as the filter bar holds them: stance, kind.
    filters: [usize; 2],
    /// How many rows the list shows.
    shown_rows: usize,
    /// The rows as arranged last; `None` when something changed.
    arranged: Option<Vec<Row>>,
}

impl ListState {
    fn new(view: View) -> ListState {
        ListState { view, filters: [0, 0], shown_rows: PAGE_ROWS, arranged: None }
    }

    /// Sorts by `column`; the column sorted already turns its direction.
    pub fn sort_by(&mut self, column: Column) {
        if self.view.sort == column {
            self.view.ascending = !self.view.ascending;
        } else {
            self.view.sort = column;
            self.view.ascending = true;
        }
        self.arranged = None;
    }
}

/// What the page keeps.
pub struct HandlePageState {
    pub search: HandleSearch,
    /// The tab shown.
    pub tab: Tab,
    /// The handles searched lately, the newest first.
    recent: Vec<(String, f64)>,
    /// The list of the relations.
    pub list: ListState,
    /// The list of the handles searched lately.
    pub recent_list: ListState,
    /// When the files were looked at last, and what they said.
    looked: f64,
    stamps: Option<Stamps>,
    /// A handle of the list was searched: the page scrolls up to the card.
    to_top: bool,
    /// An entry of a list was opened in the search this frame.
    opened: bool,
    /// The mark deleted a moment ago: since when (the time of the input), where it stood, what it was.
    removed: Option<(f64, usize, Relation)>,
    /// Tells the overlay that something changed.
    notify: fn(),
}

impl HandlePageState {
    /// The page of the data folder.
    pub fn with_files(paths: &Paths) -> HandlePageState {
        HandlePageState::with_search(HandleSearch::with_files(paths))
    }

    /// The page around `search`; the handles kept come with the first frame.
    pub fn with_search(mut search: HandleSearch) -> HandlePageState {
        // On the page the card is as wide as the cards around it.
        search.card_spread = true;
        HandlePageState {
            search,
            tab: Tab::default(),
            recent: Vec::new(),
            list: ListState::new(View::default()),
            recent_list: ListState::new(View::recent()),
            looked: f64::NEG_INFINITY,
            stamps: None,
            to_top: false,
            opened: false,
            removed: None,
            notify: tell_overlay,
        }
    }

    /// The preview: made-up relations and handles searched, nothing read and nothing asked.
    pub fn sample(now: f64) -> HandlePageState {
        let mut relations = Relations::default();
        relations.set(Kind::Handle, "NightOwl_77", Some(Stance::Friendly));
        relations.set(Kind::Org, "NOVA", Some(Stance::Friendly));
        relations.set(Kind::Handle, "QuietComet", Some(Stance::Neutral));
        relations.set(Kind::Handle, "Vex", Some(Stance::Hostile));
        relations.set(Kind::Org, "REDSUN", Some(Stance::Hostile));
        let (found, roster) = sample_card(now);
        let search = HandleSearch::empty().with_relations(relations).with_sample(found, roster);
        let mut page = HandlePageState::with_search(search);
        page.recent = vec![
            ("Drifter_9".into(), now - 600.0),
            ("NightOwl_77".into(), now - 3600.0),
            ("Vex".into(), now - 172_800.0),
        ];
        page.looked = f64::INFINITY;
        page
    }

    /// Replaces what tells the overlay (tests count it).
    pub fn with_notify(mut self, notify: fn()) -> HandlePageState {
        self.notify = notify;
        self
    }

    /// The list `which`.
    pub fn list_of(&mut self, which: Which) -> &mut ListState {
        match which {
            Which::Relations => &mut self.list,
            Which::Recent => &mut self.recent_list,
        }
    }

    /// The rows the list of the relations shows, all of them, in their order.
    pub fn arranged(&mut self) -> &[Row] {
        self.arranged_of(Which::Relations)
    }

    /// The rows the list `which` shows, all of them, in their order.
    pub fn arranged_of(&mut self, which: Which) -> &[Row] {
        if self.list_of(which).arranged.is_none() {
            let rows = rows(self.search.relations(), &self.recent);
            let list = self.list_of(which);
            list.arranged = Some(arrange(&rows, &list.view));
        }
        self.list_of(which).arranged.as_deref().unwrap_or_default()
    }

    /// Both lists are arranged anew.
    fn rearrange(&mut self) {
        self.list.arranged = None;
        self.recent_list.arranged = None;
    }

    /// Reads the relations, the settings and the handles kept anew when one of their files was written since
    /// the last look; looks at most once in [`LOOK_SECONDS`].
    pub fn refresh(&mut self, now: f64) {
        if now - self.looked < LOOK_SECONDS {
            return;
        }
        self.looked = now;
        let Some(paths) = self.search.data_paths() else { return };
        let stamps = Stamps::of(paths);
        if self.stamps.as_ref() == Some(&stamps) {
            return;
        }
        self.recent = recent_of(paths);
        if self.stamps.is_some() {
            self.search.reload_settings();
        }
        self.stamps = Some(stamps);
        self.rearrange();
    }

    /// Sorts the list of the relations by `column`; the column sorted already turns its direction.
    pub fn sort_by(&mut self, column: Column) {
        self.list.sort_by(column);
    }

    /// Something was written: the overlay is told and the lists arranged anew.
    fn after_change(&mut self) {
        if self.search.take_changed() {
            (self.notify)();
            self.rearrange();
        }
    }
}

/// A cell `width` wide in a row, its content from the left, cut at its edge.
fn cell<R>(ui: &mut Ui, width: f32, add: impl FnOnce(&mut Ui) -> R) -> R {
    let height = widgets::ICON_BUTTON_SIZE;
    ui.allocate_ui_with_layout(egui::vec2(width, height), Layout::left_to_right(Align::Center), |ui| {
        ui.set_width(width);
        add(ui)
    })
    .inner
}

/// The width of the marks and the button that takes the mark away.
fn actions_width(ui: &Ui) -> f32 {
    let gap = ui.spacing().item_spacing.x;
    3.0 * MARK_SIZE + 3.0 * gap + widgets::ICON_BUTTON_SIZE
}

/// The widths of the columns stance, kind, name and searched in a row of `ui`: the three beside the name shrink
/// down to half their width before the name gets narrower than [`NAME_MIN_WIDTH`].
fn column_widths(ui: &Ui) -> [f32; 4] {
    let gap = ui.spacing().item_spacing.x;
    let rest = actions_width(ui) + 4.0 * gap;
    let wanted = STANCE_WIDTH + KIND_WIDTH + SEARCHED_WIDTH;
    let room = ui.available_width() - rest - NAME_MIN_WIDTH;
    let shrink = (room / wanted).clamp(0.5, 1.0);
    let (stance, kind, searched) = (STANCE_WIDTH * shrink, KIND_WIDTH * shrink, SEARCHED_WIDTH * shrink);
    let name = (ui.available_width() - rest - stance - kind - searched).max(NAME_MIN_WIDTH);
    [stance, kind, name, searched]
}

/// The colour, the icon and the word of a stance; unknown in the muted colour.
fn stance_look(scheme: &Scheme, stance: Option<Stance>) -> (Color, &'static str, String) {
    match stance {
        Some(stance) => {
            (handle_search::stance_color(scheme, stance), handle_search::stance_icon(stance), t(stance.label()))
        }
        None => (scheme.sub, UNKNOWN_ICON, t("s_hqs_unbekannt")),
    }
}

fn small(ui: &mut Ui, text: &str, tone: Color) -> egui::Response {
    ui.add(egui::Label::new(RichText::new(text).small().color(color(tone))).truncate())
}

/// The head of the list `which`: the columns, each sorting on a click.
fn head(ui: &mut Ui, scheme: &Scheme, state: &mut HandlePageState, which: Which) {
    let view = state.list_of(which).view.clone();
    let sorted = |column: Column| (view.sort == column).then_some(view.ascending);
    let mut clicked = None;
    ui.horizontal(|row| {
        let [stance_width, kind_width, name_width, searched_width] = column_widths(row);
        for (column, width, key) in [
            (Column::Stance, stance_width, "s_hqs_haltung"),
            (Column::Kind, kind_width, "s_hqs_art"),
            (Column::Name, name_width, "s_hqs_name"),
            (Column::Searched, searched_width, "s_hqs_gesucht"),
        ] {
            cell(row, width, |c| {
                if widgets::sort_head(c, scheme, &t(key), sorted(column), false) {
                    clicked = Some(column);
                }
            });
        }
    });
    if let Some(column) = clicked {
        state.list_of(which).sort_by(column);
    }
}

/// One row: stance, kind, name, time searched, then the marks and, for a row marked, the button that deletes the
/// mark. A click on the name of a handle searches it, on that of an organization opens its card.
fn row(ui: &mut Ui, scheme: &Scheme, state: &mut HandlePageState, entry: &Row, now: f64) {
    let (tone, icon, word) = stance_look(scheme, entry.stance);
    ui.horizontal(|row| {
        let [stance_width, kind_width, name_width, searched_width] = column_widths(row);
        cell(row, stance_width, |c| {
            if let Some(image) = crate::icons::image(icon, tone, MARK_SIZE) {
                c.add(image);
            }
            c.add(egui::Label::new(RichText::new(&word).color(color(tone))).truncate());
        });
        cell(row, kind_width, |c| {
            let kind = t(match entry.kind {
                Kind::Handle => "s_hqs_handle",
                Kind::Org => "s_hqs_org",
            });
            small(c, &kind, scheme.sub);
        });
        cell(row, name_width, |c| {
            let text =
                RichText::new(&entry.name).strong().color(color(if entry.stance.is_some() { tone } else { scheme.fg }));
            let hint = match entry.kind {
                Kind::Handle => tf("s_hqs_suchen", &[("handle", Arg::Text(entry.name.clone()))]),
                Kind::Org => tf("s_hqs_org_oeffnen", &[("sid", Arg::Text(entry.name.clone()))]),
            };
            let response = c.add(egui::Label::new(text).truncate().sense(Sense::click()));
            response.widget_info(|| WidgetInfo::labeled(Role::Button, true, &entry.name));
            if response.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(hint).clicked() {
                match entry.kind {
                    Kind::Handle => state.search.search(c.ctx(), &entry.name, false),
                    Kind::Org => state.search.open_org(c.ctx(), &entry.name),
                }
                state.to_top = true;
                state.opened = true;
            }
        });
        cell(row, searched_width, |c| {
            let age = entry.searched.map_or_else(|| "—".to_string(), |at| language::relative_age(now - at));
            small(c, &age, scheme.sub);
        });
        cell(row, actions_width(row), |c| {
            state.search.marks(c, scheme, entry.kind, &entry.name);
            if entry.stance.is_some() {
                let meaning = tf("s_hqs_entfernen", &[("name", Arg::Text(entry.name.clone()))]);
                if widgets::icon_button(c, scheme, "trash-2", &meaning).clicked()
                    && let Some((at, gone)) = state.search.remove_relation(entry.kind, &entry.name)
                {
                    state.removed = Some((c.input(|i| i.time), at, gone));
                }
            }
        });
    });
}

/// The filters of the list of the relations: a text, the stance and the kind.
fn filters(ui: &mut Ui, scheme: &Scheme, state: &mut HandlePageState) {
    let list = &mut state.list;
    ui.horizontal_wrapped(|row| {
        if widgets::search_field(row, &mut list.view.text, &t("s_hqs_filter"), scheme, 180.0).changed() {
            list.arranged = None;
        }
        let stances = [
            t("s_hqs_alle_markierten"),
            t(Stance::Friendly.label()),
            t(Stance::Neutral.label()),
            t("s_hqs_unbekannt"),
            t(Stance::Hostile.label()),
        ];
        let kinds = [t("s_hqs_alle_arten"), t("s_hqs_handles"), t("s_hqs_orgs")];
        let stance_refs: Vec<&str> = stances.iter().map(String::as_str).collect();
        let kind_refs: Vec<&str> = kinds.iter().map(String::as_str).collect();
        if widgets::filter_bar(row, "handle-liste", &[&stance_refs, &kind_refs], &mut list.filters) {
            list.view.stance = list.filters[0];
            list.view.kind = list.filters[1];
            list.arranged = None;
        }
    });
}

/// The list `which` with its head and the rows shown; the relations with their filters above. `empty` says what
/// stands in place of a list without a row before any filter.
fn list(ui: &mut Ui, scheme: &Scheme, state: &mut HandlePageState, now: f64, which: Which, empty: &str) {
    let nothing = match which {
        Which::Relations => state.search.relations().is_empty() && state.recent.is_empty(),
        Which::Recent => state.recent.is_empty(),
    };
    if nothing {
        widgets::empty_state(ui, scheme, "users", empty);
        return;
    }
    if which == Which::Relations {
        filters(ui, scheme, state);
        ui.add_space(4.0);
    }
    head(ui, scheme, state, which);
    let shown_rows = state.list_of(which).shown_rows;
    let total = state.arranged_of(which).len();
    let shown: Vec<Row> = state.arranged_of(which).iter().take(shown_rows).cloned().collect();
    if shown.is_empty() {
        // No relation yet: the list says how to set one; else nothing fits the filters.
        let marked_only = which == Which::Relations && state.list.view.stance == 0;
        if marked_only && state.search.relations().is_empty() {
            widgets::empty_state(ui, scheme, "users", empty);
        } else {
            widgets::hint(ui, scheme, &t("s_hqs_keine_treffer"));
        }
        return;
    }
    widgets::list_frame(ui, scheme, |ui| {
        ui.set_min_width(ui.available_width());
        for entry in &shown {
            row(ui, scheme, state, entry, now);
        }
    });
    let left = total.saturating_sub(shown_rows);
    if left > 0 {
        ui.add_space(4.0);
        let word = tf("s_hqs_mehr", &[("n", Arg::Int(left.min(PAGE_ROWS) as i64))]);
        if ui.add(widgets::Button::new(&word, ButtonKind::Normal, scheme)).clicked() {
            state.list_of(which).shown_rows += PAGE_ROWS;
        }
    }
}

/// The notice of a mark deleted in a list: for a few seconds its button puts the mark back at its place.
fn undo_notice(ui: &Ui, scheme: &Scheme, state: &mut HandlePageState) {
    let Some((since, at, gone)) = state.removed.take() else { return };
    let text = tf("s_hqs_geloescht", &[("name", Arg::Text(gone.name.clone()))]);
    let shown = widgets::notice(ui.ctx(), scheme, since, "trash-2", &text, Some(&t("s_mlg_rueckgaengig")));
    if shown.acted {
        state.search.restore_relation(at, gone);
    } else if shown.shown {
        state.removed = Some((since, at, gone));
    }
}

/// A card with `title` over `add`.
fn titled_card(ui: &mut Ui, scheme: &Scheme, title: &str, add: impl FnOnce(&mut Ui)) {
    widgets::card(ui, scheme, |ui| {
        widgets::card_title(ui, scheme, title);
        ui.add_space(4.0);
        add(ui);
    });
}

/// The tab "Suche": the field, the card of the handle found and the handles searched lately.
fn search_tab(ui: &mut Ui, scheme: &Scheme, state: &mut HandlePageState, now: f64) {
    widgets::hint(ui, scheme, &t("s_hqs_lead"));
    ui.add_space(6.0);
    let mut field = None;
    titled_card(ui, scheme, &t("s_hqt_karte_suche"), |ui| field = Some(state.search.field(ui, scheme)));
    if std::mem::take(&mut state.to_top)
        && let Some(field) = &field
    {
        field.scroll_to_me(Some(Align::TOP));
    }
    ui.add_space(GAP);
    // The card of the handle keeps its own width without narrowing the page below it. The members of an
    // organization stand in the card, as in the info bar; a member clicked shows at the top.
    let searching = state.search.searching();
    let card = ui.scope(|ui| state.search.card(ui, scheme)).response.rect;
    state.to_top |= !searching && state.search.searching();
    if card.height() > 0.5 {
        ui.add_space(GAP);
    }
    titled_card(ui, scheme, &t("s_hq_verlauf"), |ui| {
        widgets::hint(ui, scheme, &t("s_hqs_verlauf_h"));
        ui.add_space(4.0);
        list(ui, scheme, state, now, Which::Recent, &t("s_hqs_verlauf_leer"));
    });
    state.opened = false;
    ui.add_space(GAP);
    // The settings of the search stand with the info bar, in one place with its other settings.
    widgets::hint(ui, scheme, &t("s_hqs_einstellungen_wo"));
}

/// The tab "Beziehungen": the export and the import, and the handles and organizations marked. An entry opened
/// goes to the tab of the search.
fn relations_tab(ui: &mut Ui, scheme: &Scheme, state: &mut HandlePageState, now: f64) {
    widgets::hint(ui, scheme, &t("s_hqb_lead"));
    ui.add_space(6.0);
    titled_card(ui, scheme, &t("s_hqt_karte_austausch"), |ui| state.search.transfer(ui, scheme));
    ui.add_space(GAP);
    titled_card(ui, scheme, &t("s_hqs_liste"), |ui| {
        widgets::hint(ui, scheme, &t("s_hqs_liste_h"));
        ui.add_space(4.0);
        list(ui, scheme, state, now, Which::Relations, &t("s_hqs_leer"));
    });
    if std::mem::take(&mut state.opened) {
        state.tab = Tab::Search;
    }
}

/// Draws the page "Handle-Suche": the tabs "Suche" and "Beziehungen" and the one chosen.
pub fn show_page(ui: &mut Ui, scheme: &Scheme, state: &mut HandlePageState) {
    let now = handle_search::now();
    state.refresh(now);
    let labels: Vec<String> = Tab::ALL.iter().map(|tab| tab.label()).collect();
    let names: Vec<&str> = labels.iter().map(String::as_str).collect();
    let mut chosen = state.tab.index();
    if widgets::tab_bar(ui, scheme, &names, &mut chosen) {
        state.tab = Tab::ALL.get(chosen).copied().unwrap_or_default();
    }
    ui.add_space(GAP);
    match state.tab {
        Tab::Search => search_tab(ui, scheme, state, now),
        Tab::Relations => relations_tab(ui, scheme, state, now),
    }
    undo_notice(ui, scheme, state);
    state.after_change();
}

/// The settings of the search, for the settings of the info bar: written changes reach the overlay.
pub fn show_settings(ui: &mut Ui, scheme: &Scheme, state: &mut HandlePageState) {
    state.refresh(handle_search::now());
    state.search.settings(ui, scheme);
    state.after_change();
}

#[cfg(test)]
mod tests;
