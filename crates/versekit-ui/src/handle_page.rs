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

//! The page "Handle-Suche" of the main window: the field and the card of the handle search at the top, below
//! them the list of the handles and organizations marked, together with the handles searched lately (without a
//! mark they count as unknown), and the settings of the search at the bottom. The members of an organization
//! opened on the card stand in the card, as in the info bar.
//!
//! The list sorts by stance, kind, name or the time searched and filters by stance, kind and a text. A click on
//! a handle searches it at the top; the marks of a row set or take away its stance.
//!
//! The overlay runs in a process of its own and keeps the relations and settings in memory: after every change
//! here it is told to read them anew. A change the overlay wrote shows here within a second, from the times the
//! files were written.

use std::time::SystemTime;

use egui::{Align, Layout, RichText, Role, Sense, Ui, WidgetInfo};
use versekit_core::handle_query::members::{Member, Roster};
use versekit_core::handle_query::relations::{self, Kind, Relations, Stance};
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

/// The choices of the stance filter: all, then one stance each, `Some(None)` for unknown.
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
    /// An index into the stance filter: 0 all, 1 friendly, 2 neutral, 3 unknown, 4 hostile.
    pub stance: usize,
    /// An index into the kind filter: 0 all, 1 handles, 2 organizations.
    pub kind: usize,
    /// A part of the name, capitals and small letters alike.
    pub text: String,
}

impl Default for View {
    fn default() -> View {
        View { sort: Column::Stance, ascending: true, stance: 0, kind: 0, text: String::new() }
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
    let mut shown: Vec<(String, &Row)> = rows
        .iter()
        .filter(|row| stance.is_none_or(|s| row.stance == s))
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

/// The handles kept, the newest first, as the site writes them.
fn recent_of(paths: &Paths) -> Vec<(String, f64)> {
    cache::read(paths).into_iter().rev().map(|(_, card, at)| (card.profile.handle, at)).collect()
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

/// What the page keeps.
pub struct HandlePageState {
    pub search: HandleSearch,
    /// The handles searched lately, the newest first.
    recent: Vec<(String, f64)>,
    pub view: View,
    /// The filters as the filter bar holds them: stance, kind.
    filters: [usize; 2],
    /// How many rows the list shows.
    shown_rows: usize,
    /// The rows as arranged last; `None` when something changed.
    arranged: Option<Vec<Row>>,
    /// When the files were looked at last, and what they said.
    looked: f64,
    stamps: Option<Stamps>,
    /// A handle of the list was searched: the page scrolls up to the card.
    to_top: bool,
    /// Tells the overlay that something changed.
    notify: fn(),
}

impl HandlePageState {
    /// The page of the data folder.
    pub fn with_files(paths: &Paths) -> HandlePageState {
        HandlePageState::with_search(HandleSearch::with_files(paths))
    }

    /// The page around `search`; the handles kept come with the first frame.
    pub fn with_search(search: HandleSearch) -> HandlePageState {
        HandlePageState {
            search,
            recent: Vec::new(),
            view: View::default(),
            filters: [0, 0],
            shown_rows: PAGE_ROWS,
            arranged: None,
            looked: f64::NEG_INFINITY,
            stamps: None,
            to_top: false,
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
        page.recent =
            vec![("Drifter_9".into(), now - 600.0), ("NightOwl_77".into(), now - 3600.0), ("Vex".into(), now - 172_800.0)];
        page.looked = f64::INFINITY;
        page
    }

    /// Replaces what tells the overlay (tests count it).
    pub fn with_notify(mut self, notify: fn()) -> HandlePageState {
        self.notify = notify;
        self
    }

    /// The rows the list shows, all of them, in their order.
    pub fn arranged(&mut self) -> &[Row] {
        if self.arranged.is_none() {
            self.arranged = Some(arrange(&rows(self.search.relations(), &self.recent), &self.view));
        }
        self.arranged.as_deref().unwrap_or_default()
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
        self.arranged = None;
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

    /// Something was written: the overlay is told and the list arranged anew.
    fn after_change(&mut self) {
        if self.search.take_changed() {
            (self.notify)();
            self.arranged = None;
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

/// The width of the name column in a row of `ui`.
fn name_width(ui: &Ui) -> f32 {
    let gap = ui.spacing().item_spacing.x;
    let fixed = STANCE_WIDTH + KIND_WIDTH + SEARCHED_WIDTH + actions_width(ui) + 4.0 * gap;
    (ui.available_width() - fixed).max(NAME_MIN_WIDTH)
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

/// The head of the list: the columns, each sorting on a click.
fn head(ui: &mut Ui, scheme: &Scheme, state: &mut HandlePageState) {
    let sorted = |column: Column| (state.view.sort == column).then_some(state.view.ascending);
    let mut clicked = None;
    ui.horizontal(|row| {
        let name_width = name_width(row);
        for (column, width, key) in [
            (Column::Stance, STANCE_WIDTH, "s_hqs_haltung"),
            (Column::Kind, KIND_WIDTH, "s_hqs_art"),
            (Column::Name, name_width, "s_hqs_name"),
            (Column::Searched, SEARCHED_WIDTH, "s_hqs_gesucht"),
        ] {
            cell(row, width, |c| {
                if widgets::sort_head(c, scheme, &t(key), sorted(column), false) {
                    clicked = Some(column);
                }
            });
        }
    });
    if let Some(column) = clicked {
        state.sort_by(column);
    }
}

/// One row: stance, kind, name, time searched, then the marks and, for a row marked, the button that takes the
/// mark away. A click on the name of a handle searches it.
fn row(ui: &mut Ui, scheme: &Scheme, state: &mut HandlePageState, entry: &Row, now: f64) {
    let (tone, icon, word) = stance_look(scheme, entry.stance);
    ui.horizontal(|row| {
        let name_width = name_width(row);
        cell(row, STANCE_WIDTH, |c| {
            if let Some(image) = crate::icons::image(icon, tone, MARK_SIZE) {
                c.add(image);
            }
            c.add(egui::Label::new(RichText::new(&word).color(color(tone))).truncate());
        });
        cell(row, KIND_WIDTH, |c| {
            let kind = t(match entry.kind {
                Kind::Handle => "s_hqs_handle",
                Kind::Org => "s_hqs_org",
            });
            small(c, &kind, scheme.sub);
        });
        cell(row, name_width, |c| {
            let text = RichText::new(&entry.name).strong().color(color(if entry.stance.is_some() {
                tone
            } else {
                scheme.fg
            }));
            match entry.kind {
                Kind::Handle => {
                    let hint = tf("s_hqs_suchen", &[("handle", Arg::Text(entry.name.clone()))]);
                    let response = c.add(egui::Label::new(text).truncate().sense(Sense::click()));
                    response.widget_info(|| WidgetInfo::labeled(Role::Button, true, &entry.name));
                    if response.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(hint).clicked() {
                        state.search.search(c.ctx(), &entry.name, false);
                        state.to_top = true;
                    }
                }
                Kind::Org => {
                    c.add(egui::Label::new(text).truncate());
                }
            }
        });
        cell(row, SEARCHED_WIDTH, |c| {
            let age = entry.searched.map_or_else(|| "—".to_string(), |at| language::relative_age(now - at));
            small(c, &age, scheme.sub);
        });
        cell(row, actions_width(row), |c| {
            state.search.marks(c, scheme, entry.kind, &entry.name);
            if entry.stance.is_some() {
                let meaning = tf("s_hqs_entfernen", &[("name", Arg::Text(entry.name.clone()))]);
                if widgets::icon_button(c, scheme, "trash-2", &meaning).clicked() {
                    state.search.set_stance(entry.kind, &entry.name, None);
                }
            }
        });
    });
}

/// The list with its filters, its head and the rows shown.
fn list(ui: &mut Ui, scheme: &Scheme, state: &mut HandlePageState, now: f64) {
    widgets::heading(ui, scheme, &t("s_hqs_liste"));
    widgets::hint(ui, scheme, &t("s_hqs_liste_h"));
    ui.add_space(4.0);
    if state.search.relations().is_empty() && state.recent.is_empty() {
        widgets::empty_state(ui, scheme, "users", &t("s_hqs_leer"));
        return;
    }
    ui.horizontal_wrapped(|row| {
        if widgets::search_field(row, &mut state.view.text, &t("s_hqs_filter"), scheme, 180.0).changed() {
            state.arranged = None;
        }
        let stances = [
            t("s_hqs_alle_haltungen"),
            t(Stance::Friendly.label()),
            t(Stance::Neutral.label()),
            t("s_hqs_unbekannt"),
            t(Stance::Hostile.label()),
        ];
        let kinds = [t("s_hqs_alle_arten"), t("s_hqs_handles"), t("s_hqs_orgs")];
        let stance_refs: Vec<&str> = stances.iter().map(String::as_str).collect();
        let kind_refs: Vec<&str> = kinds.iter().map(String::as_str).collect();
        if widgets::filter_bar(row, "handle-liste", &[&stance_refs, &kind_refs], &mut state.filters) {
            state.view.stance = state.filters[0];
            state.view.kind = state.filters[1];
            state.arranged = None;
        }
    });
    ui.add_space(4.0);
    head(ui, scheme, state);
    let shown_rows = state.shown_rows;
    let total = state.arranged().len();
    let shown: Vec<Row> = state.arranged().iter().take(shown_rows).cloned().collect();
    if shown.is_empty() {
        widgets::hint(ui, scheme, &t("s_hqs_keine_treffer"));
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
            state.shown_rows += PAGE_ROWS;
        }
    }
}

/// Draws the page.
pub fn show_page(ui: &mut Ui, scheme: &Scheme, state: &mut HandlePageState) {
    let now = handle_search::now();
    state.refresh(now);
    widgets::hint(ui, scheme, &t("s_hqs_lead"));
    ui.add_space(6.0);
    let field = state.search.field(ui, scheme);
    if std::mem::take(&mut state.to_top) {
        field.scroll_to_me(Some(Align::TOP));
    }
    // The card keeps its own width without narrowing the page below it. The members of an organization stand in
    // the card, as in the info bar; a member clicked shows at the top.
    let searching = state.search.searching();
    ui.scope(|ui| state.search.card(ui, scheme));
    state.to_top |= !searching && state.search.searching();
    ui.add_space(GAP);
    list(ui, scheme, state, now);
    state.after_change();
    ui.add_space(GAP);
    // The settings of the search stand with the info bar, in one place with its other settings.
    widgets::hint(ui, scheme, &t("s_hqs_einstellungen_wo"));
}

/// The settings of the search, for the settings of the info bar: written changes reach the overlay.
pub fn show_settings(ui: &mut Ui, scheme: &Scheme, state: &mut HandlePageState) {
    state.refresh(handle_search::now());
    state.search.settings(ui, scheme);
    state.after_change();
}

#[cfg(test)]
mod tests;
