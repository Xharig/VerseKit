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

//! The route view of the selling page: the chosen goods sold over a few
//! stations in a row (`versekit_core::sell_route`).
//!
//! The settings (system, most stations, start, stations that count as full)
//! sit with the goods, the route in the list. A station can be reported full
//! or sold at; the route is then calculated again for what is left. Every
//! calculation runs in a job, the page shows a spinner meanwhile.

use std::collections::HashMap;
use std::sync::Arc;

use egui::{Align, Layout, RichText, Ui};
use versekit_core::formats;
use versekit_core::language::{self, Arg, Lang, t, tf};
use versekit_core::paths::Paths;
use versekit_core::routes;
use versekit_core::sell_route::{self, Distances, Query, Route, Stop};
use versekit_core::selling::Selling;
use versekit_core::theme::Scheme;

use crate::color;
use crate::layout::{self, GAP};
use crate::trade::{self, Job, Polled};
use crate::widgets::{self, ButtonKind};

/// The stations a route has at most, as the buttons offer them.
const MOST_STOPS: usize = 6;
const DEFAULT_STOPS: usize = 4;

/// A station of the route the player has sold at already.
#[derive(Clone, Debug, PartialEq)]
struct Done {
    terminal: String,
    name: String,
    sold: Vec<(String, f64)>,
}

/// What a calculation was made for: the query and the version of the stored prices.
#[derive(Clone, Debug, PartialEq)]
struct Request {
    query: Query,
    version: u64,
}

/// What the route view holds.
pub(crate) struct RouteState {
    /// The system chosen: `0` all, else the place in the list of systems.
    system: usize,
    stops: usize,
    start: Option<(String, String)>,
    start_text: String,
    include_full: bool,
    /// Stations the player reported full: number and name.
    blocked: Vec<(String, String)>,
    done: Vec<Done>,
    job: Option<(Request, Job<Route>)>,
    result: Option<(Request, Route)>,
}

impl Default for RouteState {
    fn default() -> Self {
        RouteState {
            system: 0,
            stops: DEFAULT_STOPS,
            start: None,
            start_text: String::new(),
            include_full: false,
            blocked: Vec::new(),
            done: Vec::new(),
            job: None,
            result: None,
        }
    }
}

/// What the route is calculated from.
pub(crate) struct Inputs<'a> {
    pub data: &'a Selling,
    pub picks: &'a [String],
    /// The amounts in SCU of the goods.
    pub amounts: &'a HashMap<String, f64>,
    pub nqa: bool,
    /// The data folder; without it the route is calculated at once and without distances.
    pub paths: Option<&'a Arc<Paths>>,
    /// Counts the reloads of the prices.
    pub version: u64,
}

/// What a click on a card asks for.
enum Act {
    Full(usize),
    Sold(usize),
    Unblock(usize),
    Undo(usize),
    Reset,
}

impl RouteState {
    /// The systems with the number of their trading posts, the largest first.
    fn systems(data: &Selling) -> Vec<(String, usize)> {
        let mut counts: Vec<(String, usize)> = Vec::new();
        for (ident, _) in routes::trade_posts(data) {
            let system = data.terminals.get(&ident).map(|s| s.system.clone()).unwrap_or_default();
            if system.is_empty() {
                continue;
            }
            match counts.iter_mut().find(|(name, _)| *name == system) {
                Some(entry) => entry.1 += 1,
                None => counts.push((system, 1)),
            }
        }
        counts.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        counts
    }

    /// The query for the goods chosen, without what was sold already; `None` where nothing is left to sell.
    fn request(&self, inputs: &Inputs<'_>) -> Option<Request> {
        let items: Vec<(String, f64)> = inputs
            .picks
            .iter()
            .filter_map(|good| {
                let held = inputs.amounts.get(good).copied().unwrap_or(0.0);
                let sold: f64 = self
                    .done
                    .iter()
                    .flat_map(|done| done.sold.iter())
                    .filter(|(name, _)| name == good)
                    .map(|(_, scu)| *scu)
                    .sum();
                let left = held - sold;
                (left > 1.0e-9).then(|| (good.clone(), left))
            })
            .collect();
        if items.is_empty() {
            return None;
        }
        let systems = Self::systems(inputs.data);
        let system = self.system.checked_sub(1).and_then(|i| systems.get(i)).map(|(name, _)| name.clone());
        let start = self
            .done
            .last()
            .map(|done| done.terminal.clone())
            .or_else(|| self.start.as_ref().map(|(ident, _)| ident.clone()));
        // The stations sold at are no candidates again, and no start of the same place twice.
        let mut blocked: Vec<String> = self.blocked.iter().map(|(ident, _)| ident.clone()).collect();
        blocked.extend(self.done.iter().map(|done| done.terminal.clone()));
        Some(Request {
            query: Query {
                items,
                system: system.unwrap_or_default(),
                nqa_only: inputs.nqa,
                include_full: self.include_full,
                max_stops: self.stops,
                start,
                blocked,
            },
            version: inputs.version,
        })
    }

    /// Takes in a finished calculation.
    pub(crate) fn poll(&mut self, ui: &Ui) {
        let Some((_, job)) = &self.job else { return };
        match job.poll() {
            Polled::Running => {}
            Polled::Done(route) => {
                if let Some((request, _)) = self.job.take() {
                    self.result = Some((request, route));
                }
                ui.ctx().request_repaint();
            }
            Polled::Lost => self.job = None,
        }
    }

    /// Starts the calculation that the settings ask for, unless it is done or running already.
    fn ensure(&mut self, ctx: &egui::Context, inputs: &Inputs<'_>) -> Option<Request> {
        let Some(request) = self.request(inputs) else {
            self.result = None;
            self.job = None;
            return None;
        };
        if self.result.as_ref().is_some_and(|(done, _)| *done == request)
            || self.job.as_ref().is_some_and(|(asked, _)| *asked == request)
        {
            return Some(request);
        }
        match inputs.paths {
            Some(paths) => {
                let paths = paths.clone();
                let data = inputs.data.clone();
                let query = request.query.clone();
                self.job = Some((
                    request.clone(),
                    Job::spawn(ctx, "selling-route", move || {
                        sell_route::calculate(&paths, &data, &query, trade::now(), &trade::fetcher())
                    }),
                ));
            }
            None => {
                let route = sell_route::plan(inputs.data, &request.query, &Distances::default());
                self.result = Some((request.clone(), route));
            }
        }
        Some(request)
    }

    /// Forgets the stations reported full and the goods sold already.
    fn reset(&mut self) {
        self.blocked.clear();
        self.done.clear();
    }

    /// Is a route being calculated?
    #[cfg(test)]
    pub(crate) fn is_calculating(&self) -> bool {
        self.job.is_some()
    }

    /// The finished route (for the tests of the page).
    #[cfg(test)]
    pub(crate) fn route(&self) -> Option<&Route> {
        self.result.as_ref().map(|(_, route)| route)
    }

    /// Reports a station full as the button on its card does (for the tests of the page).
    #[cfg(test)]
    pub(crate) fn report_full(&mut self, ident: &str, name: &str) {
        self.blocked.push((ident.to_string(), name.to_string()));
    }

    /// Takes a station as sold at, with what was sold there (for the tests of the page).
    #[cfg(test)]
    pub(crate) fn sold_at(&mut self, ident: &str, name: &str, sold: Vec<(String, f64)>) {
        self.done.push(Done { terminal: ident.to_string(), name: name.to_string(), sold });
    }
}

#[cfg(test)]
mod tests;

// ------------------------------------------------------------ the settings

/// The entries of the field for the start: what is shown and `(number, name)`.
fn start_labels(data: &Selling) -> (Vec<String>, Vec<(String, String)>) {
    let mut labels = Vec::new();
    let mut idents = Vec::new();
    for (ident, name) in routes::trade_posts(data) {
        let spot = data.terminals.get(&ident);
        let mut label = name.clone();
        for part in [spot.map(|s| s.place.as_str()), spot.map(|s| s.system.as_str())].into_iter().flatten() {
            if !part.is_empty() && part != name {
                label = format!("{label} · {part}");
            }
        }
        labels.push(label);
        idents.push((ident, name));
    }
    (labels, idents)
}

/// The small heading of a group of buttons.
fn group_title(ui: &mut Ui, scheme: &Scheme, text: &str) {
    ui.add(egui::Label::new(widgets::bold(ui, text, egui::TextStyle::Small).color(color(scheme.sub))).wrap());
}

/// The settings of the route: system, most stations, start, full stations.
pub(crate) fn controls(ui: &mut Ui, scheme: &Scheme, state: &mut RouteState, inputs: &Inputs<'_>) {
    trade::quiet(ui, scheme, &t("s_vk_ro_lead"));
    ui.add_space(4.0);
    let systems = RouteState::systems(inputs.data);
    let columns = if systems.is_empty() { 1 } else { 2 };
    // The columns stop short of the edge: snapped to whole pixels they would stick out by a fraction.
    ui.scope(|ui| {
        ui.set_max_width(ui.available_width() - layout::SAFETY);
        layout::columns_or_stack(ui, 240.0, columns, |ui, i| route_setting(ui, scheme, state, &systems, columns, i));
    });
    ui.add_space(4.0);
    start_field(ui, scheme, state, inputs.data);
    ui.add_space(4.0);
    widgets::switch_row(ui, scheme, &t("s_vk_ro_voll_trotzdem"), &mut state.include_full);
}

/// One of the two settings beside each other: the system, or the number of stations.
fn route_setting(
    ui: &mut Ui,
    scheme: &Scheme,
    state: &mut RouteState,
    systems: &[(String, usize)],
    columns: usize,
    i: usize,
) {
    if columns == 2 && i == 0 {
        group_title(ui, scheme, &t("s_vk_ro_system"));
        let mut words = vec![t("s_rt_alle_systeme")];
        words.extend(systems.iter().map(|(name, count)| format!("{name} ({count})")));
        let refs: Vec<&str> = words.iter().map(String::as_str).collect();
        let mut chosen = [state.system.min(words.len() - 1)];
        if widgets::filter_bar(ui, "verkauf-route-system", &[&refs], &mut chosen) {
            state.system = chosen[0];
        }
    } else {
        group_title(ui, scheme, &t("s_vk_ro_stationen"));
        ui.horizontal_wrapped(|row| {
            for stops in 1..=MOST_STOPS {
                let kind = if state.stops == stops { ButtonKind::Strong } else { ButtonKind::Normal };
                let word = tf("s_vk_ro_stationen_n", &[("n", Arg::from(stops))]);
                if row.add(widgets::Button::new(&word, kind, scheme)).clicked() {
                    state.stops = stops;
                }
            }
        });
    }
}

fn start_field(ui: &mut Ui, scheme: &Scheme, state: &mut RouteState, data: &Selling) {
    group_title(ui, scheme, &t("s_vk_ro_start"));
    let (labels, idents) = start_labels(data);
    let more = |n: usize| tf("s_mlg_mehr", &[("n", Arg::from(n))]);
    let hint = t("s_vk_ro_start_ph");
    let width = ui.available_width() - layout::SAFETY - widgets::ICON_BUTTON_SIZE - ui.spacing().item_spacing.x;
    let mut text = std::mem::take(&mut state.start_text);
    let mut picked = None;
    let mut clear = false;
    ui.horizontal(|row| {
        let field = widgets::SuggestField::new("verkauf-route-start", &hint, scheme, &more)
            .width(width)
            .show(row, &mut text, &labels);
        picked = field.picked;
        if state.start.is_some() {
            clear = widgets::icon_button(row, scheme, "x", &t("s_vk_ro_start_weg")).clicked();
        } else {
            widgets::icon_button_blocked(row, scheme, "x", &t("s_vk_ro_start_weg"));
        }
    });
    state.start_text = text;
    if state.start.is_some() && state.start_text.is_empty() {
        clear = true;
    }
    if let Some(index) = picked
        && let Some((ident, name)) = idents.get(index)
    {
        state.start = Some((ident.clone(), name.clone()));
        state.start_text = name.clone();
    }
    if clear {
        state.start = None;
        state.start_text.clear();
    }
}

// ------------------------------------------------------------ the route

/// A distance in Gm as the card writes it.
fn gm(lang: Lang, value: f64) -> String {
    formats::fixed(lang, value, 1)
}

/// The route in the list: what was reported and sold, then the stations in a row.
pub(crate) fn results(ui: &mut Ui, scheme: &Scheme, state: &mut RouteState, inputs: &Inputs<'_>) {
    let lang = language::current();
    let request = state.ensure(&ui.ctx().clone(), inputs);
    let mut act: Option<Act> = None;
    history(ui, scheme, state, &mut act);
    if request.is_none() {
        let key = if state.done.is_empty() { "s_vk_ro_keine_menge" } else { "s_vk_ro_keine" };
        trade::quiet(ui, scheme, &t(key));
    } else if state.job.is_some() {
        widgets::loading(ui, scheme, &t("s_vk_ro_rechnet"));
    } else if let Some((_, route)) = &state.result {
        route_view(ui, scheme, lang, route, request.as_ref().and_then(|r| r.query.start.as_deref()), &mut act);
    }
    match act {
        Some(Act::Full(index)) => {
            if let Some(stop) = state.result.as_ref().and_then(|(_, route)| route.stops.get(index)) {
                state.blocked.push((stop.terminal.clone(), stop.name.clone()));
            }
        }
        Some(Act::Sold(index)) => {
            if let Some(stop) = state.result.as_ref().and_then(|(_, route)| route.stops.get(index)) {
                let sold = stop.lines.iter().map(|line| (line.good.clone(), line.scu)).collect();
                state.done.push(Done { terminal: stop.terminal.clone(), name: stop.name.clone(), sold });
            }
        }
        Some(Act::Unblock(index)) if index < state.blocked.len() => {
            state.blocked.remove(index);
        }
        Some(Act::Undo(index)) if index < state.done.len() => {
            state.done.truncate(index);
        }
        Some(Act::Reset) => state.reset(),
        _ => {}
    }
}

/// The stations sold at and the ones reported full, each with the way back.
fn history(ui: &mut Ui, scheme: &Scheme, state: &RouteState, act: &mut Option<Act>) {
    if state.done.is_empty() && state.blocked.is_empty() {
        return;
    }
    if !state.done.is_empty() {
        group_title(ui, scheme, &t("s_vk_ro_erledigt"));
        for (index, done) in state.done.iter().enumerate() {
            ui.horizontal_wrapped(|row| {
                row.label(RichText::new(&done.name).color(color(scheme.sub)));
                let meaning = tf("s_vk_ro_zurueck", &[("name", Arg::from(done.name.as_str()))]);
                if widgets::icon_button(row, scheme, "undo-2", &meaning).clicked() {
                    *act = Some(Act::Undo(index));
                }
            });
        }
    }
    if !state.blocked.is_empty() {
        group_title(ui, scheme, &t("s_vk_ro_gesperrt"));
        for (index, (_, name)) in state.blocked.iter().enumerate() {
            ui.horizontal_wrapped(|row| {
                row.label(RichText::new(name).color(color(scheme.sub)));
                let meaning = tf("s_vk_ro_wieder", &[("name", Arg::from(name.as_str()))]);
                if widgets::icon_button(row, scheme, "x", &meaning).clicked() {
                    *act = Some(Act::Unblock(index));
                }
            });
        }
    }
    let word = t("s_vk_ro_zuruecksetzen");
    let reset = widgets::Button::new(&word, ButtonKind::Normal, scheme).icon("rotate-ccw");
    if ui.add(reset).on_hover_text(t("s_vk_ro_zuruecksetzen_tip")).clicked() {
        *act = Some(Act::Reset);
    }
    ui.add_space(GAP);
}

fn route_view(ui: &mut Ui, scheme: &Scheme, lang: Lang, route: &Route, start: Option<&str>, act: &mut Option<Act>) {
    if route.stops.is_empty() {
        trade::quiet(ui, scheme, &t("s_vk_ro_keine"));
        unsold_line(ui, scheme, route);
        return;
    }
    ui.label(
        RichText::new(tf("s_vk_ro_summe", &[("summe", Arg::from(trade::auec(lang, route.proceeds)))]))
            .strong()
            .color(color(scheme.accent)),
    );
    let plus = route.proceeds - route.best_single;
    if route.stops.len() > 1 && plus >= 0.5 {
        let text = tf("s_vk_ro_mehr", &[("plus", Arg::from(trade::auec(lang, plus)))]);
        trade::quiet(ui, scheme, &text);
    } else if route.stops.len() == 1 {
        trade::quiet(ui, scheme, &t("s_vk_ro_gleich"));
    }
    if route.distance > 0.0 {
        trade::quiet(ui, scheme, &tf("s_vk_ro_strecke_summe", &[("gm", Arg::from(gm(lang, route.distance)))]));
    }
    if route.stops.len() > 1 {
        trade::quiet(ui, scheme, &t("s_vk_ro_regel"));
    }
    unsold_line(ui, scheme, route);
    ui.add_space(GAP);
    for (index, stop) in route.stops.iter().enumerate() {
        stop_card(ui, scheme, lang, index, stop, start.is_some(), act);
        ui.add_space(GAP);
    }
}

fn unsold_line(ui: &mut Ui, scheme: &Scheme, route: &Route) {
    if route.unsold.is_empty() {
        return;
    }
    let names: Vec<&str> = route.unsold.iter().map(|(name, _)| name.as_str()).collect();
    trade::colored(ui, scheme.yellow, &tf("s_vk_ro_unverkauft", &[("waren", Arg::from(names.join(", ")))]));
}

fn right(grid: &mut Ui, add: impl FnOnce(&mut Ui)) {
    grid.with_layout(Layout::right_to_left(Align::Center), add);
}

fn stop_card(
    ui: &mut Ui,
    scheme: &Scheme,
    lang: Lang,
    index: usize,
    stop: &Stop,
    has_start: bool,
    act: &mut Option<Act>,
) {
    widgets::list_frame(ui, scheme, |ui| {
        // The rows that take the whole width (the sum at the right, the wrapping buttons) stop short of the edge.
        ui.set_width(ui.available_width() - layout::SAFETY);
        ui.horizontal_wrapped(|row| {
            row.label(RichText::new(format!("{}.", index + 1)).strong().color(color(scheme.accent)));
            row.label(RichText::new(&stop.name).strong());
            let location: Vec<&str> =
                [stop.place.as_str(), stop.system.as_str()].into_iter().filter(|s| !s.is_empty()).collect();
            if !location.is_empty() {
                row.label(RichText::new(location.join(" · ")).color(color(scheme.sub)));
            }
            if stop.nqa {
                row.label(RichText::new(t("s_vk_nqa_marke")).color(color(scheme.yellow)));
            }
        });
        distance_line(ui, scheme, lang, index, stop, has_start);
        egui::Grid::new(("verkauf-route-ort", index)).num_columns(4).spacing([14.0, 2.0]).show(ui, |grid| {
            if index == 0 {
                grid.label("");
                for key in ["s_hl_sp_menge", "s_hl_sp_je_scu", "s_hl_sp_gesamt"] {
                    right(grid, |g| {
                        g.label(RichText::new(t(key)).strong());
                    });
                }
                grid.end_row();
            }
            for line in &stop.lines {
                let (text, tone) = match line.fill {
                    Some((key, warning)) => {
                        (format!("{}  {}", line.good, t(key)), if warning { scheme.red_text() } else { scheme.yellow })
                    }
                    None => (line.good.clone(), scheme.sub),
                };
                grid.label(RichText::new(text).color(color(tone)));
                right(grid, |g| {
                    g.label(formats::amount(lang, line.scu, &t("s_vk_scu_kurz")));
                });
                right(grid, |g| {
                    g.label(trade::plain(lang, line.price));
                });
                right(grid, |g| {
                    g.label(trade::plain(lang, line.proceeds));
                });
                grid.end_row();
            }
        });
        ui.with_layout(Layout::right_to_left(Align::Min), |side| {
            let text = tf("s_vk_erloes", &[("summe", Arg::from(trade::plain(lang, stop.proceeds)))]);
            side.label(RichText::new(text).small().color(color(scheme.accent)));
        });
        ui.horizontal_wrapped(|row| {
            let (full_word, sold_word) = (t("s_vk_ro_voll"), t("s_vk_ro_verkauft"));
            let spoken_full = format!("{full_word}: {}", stop.name);
            let spoken_sold = format!("{sold_word}: {}", stop.name);
            let full = widgets::Button::new(&full_word, ButtonKind::Normal, scheme)
                .icon("ban")
                .spoken(&spoken_full);
            if row.add(full).on_hover_text(t("s_vk_ro_voll_tip")).clicked() {
                *act = Some(Act::Full(index));
            }
            let sold = widgets::Button::new(&sold_word, ButtonKind::Normal, scheme)
                .icon("check")
                .spoken(&spoken_sold);
            if row.add(sold).on_hover_text(t("s_vk_ro_verkauft_tip")).clicked() {
                *act = Some(Act::Sold(index));
            }
        });
    });
}

/// The way to a station: how far, or why that is not known.
fn distance_line(ui: &mut Ui, scheme: &Scheme, lang: Lang, index: usize, stop: &Stop, has_start: bool) {
    // The first station of a route without a start has no way to it.
    if index == 0 && !has_start {
        return;
    }
    match stop.distance {
        Some(value) => {
            let key = if index == 0 { "s_vk_ro_gm_start" } else { "s_vk_ro_gm_vorher" };
            trade::quiet(ui, scheme, &tf(key, &[("gm", Arg::from(gm(lang, value)))]));
        }
        None => {
            let response = ui.add(egui::Label::new(
                RichText::new(t("s_vk_ro_strecke_unbekannt")).small().color(color(scheme.yellow)),
            ));
            response.on_hover_text(t("s_vk_ro_strecke_tip"));
        }
    }
}
