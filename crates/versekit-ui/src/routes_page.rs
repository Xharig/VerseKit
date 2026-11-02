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

//! The page "Routen" (`pages._routes` of 4.12): where to start, how much
//! fits in the cargo hold and the purse, and the trips that pay most from
//! there, one by one and as chains of two to four trips, one way or as a
//! round trip. A search "anywhere" fetches the trips of all trading posts
//! (about a minute and a half) and lists the best chains.
//!
//! The trips of one start are fetched when the start is chosen, the chains
//! fetch the starts they pass through; both run beside the page.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::SystemTime;

use egui::{RichText, Ui};
use versekit_core::formats;
use versekit_core::language::{self, Arg, Lang, t, tf};
use versekit_core::paths::Paths;
use versekit_core::routes::{self, ChainOptions, Leg, Routes};
use versekit_core::selling::{self, Selling};
use versekit_core::ship_market::{self, Market};
use versekit_core::stock_data::{lower, py_float};
use versekit_core::theme::Scheme;

use crate::color;
use crate::layout::{self, GAP};
use crate::trade::{self, Job, Polled};
use crate::widgets::{self, ButtonKind};

/// The cargo space and the money when nothing readable is typed.
const FALLBACK_SCU: i64 = 96;
const FALLBACK_MONEY: i64 = 500_000;
/// The trips shown of one start, of which are the single ones.
const SINGLE_SHOWN: usize = 6;
const SINGLE_ASKED: usize = 8;
/// Chains shown from the start and anywhere.
const CHAINS_SHOWN: usize = 3;
const CHAINS_ANYWHERE: usize = 8;
const LOOK_SECONDS: f64 = 1.0;
const LIST_ID: &str = "handel-routen-liste";

/// What identifies a chain calculation, so that it runs again only when something changed.
#[derive(Clone, Debug, PartialEq)]
struct Key {
    start: String,
    scu: i64,
    money: i64,
    short: bool,
    stops: i64,
    round: bool,
}

type Chains = Vec<(f64, Vec<Leg>)>;

/// What the page holds.
pub struct RoutesState {
    paths: Option<Arc<Paths>>,
    selling: Selling,
    book: Routes,
    market: Market,
    seen: [Option<SystemTime>; 3],
    looked_at: f64,
    place_text: String,
    start: Option<(String, String)>,
    system: usize,
    scu_text: String,
    money_text: String,
    ship_text: String,
    maker: usize,
    ship: Option<String>,
    short: bool,
    stops: i64,
    round: bool,
    everywhere: bool,
    start_job: Option<(String, Job<bool>)>,
    chain_job: Option<(Key, Job<Chains>)>,
    chains: Option<(Key, Chains)>,
    all_job: Option<(Job<usize>, Arc<AtomicUsize>)>,
    ships_job: Option<Job<bool>>,
    ships_asked: bool,
    /// Starts whose trips could not be fetched: the page says so instead of "nothing pays".
    failed_starts: Vec<String>,
    /// Counts the reloads of the stored trips, so that a calculation runs again only when they changed.
    book_version: u64,
    everywhere_cache: Option<(EverywhereKey, AnywhereChains)>,
    /// How far the list sticks out at the right (the edge check reads it).
    list_overflow: f32,
    /// The pass in which the page was drawn last: a gap means the page was opened anew.
    shown_pass: Option<u64>,
}

/// The best chains over all starts: profit, name of the start, trips.
type AnywhereChains = Vec<(f64, String, Vec<Leg>)>;

/// What the list of all starts was calculated for: hold, money, short, stops, round trip, version of the trips.
type EverywhereKey = (i64, i64, bool, i64, bool, u64);

impl Default for RoutesState {
    fn default() -> Self {
        RoutesState {
            paths: None,
            selling: Selling::default(),
            book: Routes::default(),
            market: Market::default(),
            seen: [None, None, None],
            looked_at: f64::NEG_INFINITY,
            place_text: String::new(),
            start: None,
            system: 0,
            scu_text: "120".to_string(),
            money_text: "500000".to_string(),
            ship_text: String::new(),
            maker: 0,
            ship: None,
            short: false,
            stops: 2,
            round: false,
            everywhere: false,
            start_job: None,
            chain_job: None,
            chains: None,
            all_job: None,
            ships_job: None,
            ships_asked: false,
            failed_starts: Vec::new(),
            book_version: 0,
            everywhere_cache: None,
            list_overflow: 0.0,
            shown_pass: None,
        }
    }
}

fn modified(file: &str) -> Option<SystemTime> {
    std::fs::metadata(file).and_then(|m| m.modified()).ok()
}

/// A whole number the way 4.12 reads the fields: points and blanks go, a
/// decimal or an empty field gives `fallback`, never less than zero.
fn as_int(text: &str, fallback: i64) -> i64 {
    let cleaned: String = text.chars().filter(|c| *c != '.' && !c.is_whitespace()).collect();
    match py_float(&cleaned) {
        Some(value) if value.is_finite() => (value.trunc() as i64).max(0),
        _ => fallback,
    }
}

impl RoutesState {
    /// With own data: the price list, the trips and the ships of the data folder.
    pub fn with_files(paths: &Paths) -> RoutesState {
        let mut state = RoutesState { paths: Some(trade::job_paths(paths)), ..RoutesState::default() };
        state.reload();
        state
    }

    fn files(&self) -> Option<[String; 3]> {
        let paths = self.paths.as_ref()?;
        Some([paths.app_file(selling::CACHE), paths.app_file(routes::CACHE), paths.app_file(ship_market::CACHE)])
    }

    fn reload(&mut self) {
        let Some(paths) = self.paths.clone() else { return };
        if let Some(files) = self.files() {
            self.seen = [modified(&files[0]), modified(&files[1]), modified(&files[2])];
        }
        self.selling = Selling::load(&paths);
        self.book = Routes::load(&paths);
        self.market = Market::load(&paths);
        self.book_version += 1;
    }

    fn follow(&mut self, ui: &Ui) {
        let now = ui.input(|i| i.time);
        if now - self.looked_at < LOOK_SECONDS {
            return;
        }
        self.looked_at = now;
        if let Some(files) = self.files()
            && self.seen != [modified(&files[0]), modified(&files[1]), modified(&files[2])]
        {
            self.reload();
        }
    }

    /// How far the list stuck out at the right in the last frame.
    pub fn list_overflow(&self) -> f32 {
        self.list_overflow
    }

    /// Is something chosen or typed that "Reset" would undo?
    pub fn is_narrowed(&self) -> bool {
        self.start.is_some()
            || !self.place_text.is_empty()
            || self.system != 0
            || self.ship.is_some()
            || !self.ship_text.is_empty()
            || self.maker != 0
            || self.short
            || self.stops != 2
            || self.round
            || self.everywhere
            || self.scu_text != "120"
            || self.money_text != "500000"
    }

    /// Back to the start: the stored trips stay.
    pub fn reset(&mut self) {
        let jobs = (self.start_job.take(), self.chain_job.take(), self.all_job.take(), self.ships_job.take());
        let ships_asked = self.ships_asked;
        let (paths, selling, book, market, seen) = (
            self.paths.take(),
            std::mem::take(&mut self.selling),
            std::mem::take(&mut self.book),
            std::mem::take(&mut self.market),
            self.seen,
        );
        *self = RoutesState { paths, selling, book, market, seen, ships_asked, ..RoutesState::default() };
        // A running search of all posts goes on; its result is read when it is done.
        self.all_job = jobs.2;
    }

    fn scu(&self) -> i64 {
        as_int(&self.scu_text, FALLBACK_SCU)
    }

    fn money(&self) -> i64 {
        as_int(&self.money_text, FALLBACK_MONEY)
    }

    fn systems(&self, posts: &[(String, String)]) -> Vec<(String, usize)> {
        let mut counts: Vec<(String, usize)> = Vec::new();
        for (ident, _) in posts {
            let system = self.selling.terminals.get(ident).map(|s| s.system.clone()).unwrap_or_default();
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

    /// The text of the field for the cargo space (for the tests of the page).
    #[cfg(test)]
    pub(crate) fn scu_text(&self) -> &str {
        &self.scu_text
    }

    /// Chooses a ship of the list as a click on its suggestion does (for the tests of the page).
    #[cfg(test)]
    pub(crate) fn pick_ship(&mut self, name: &str) {
        let scu = self.market.scu(name);
        self.set_ship(name, scu);
    }

    /// Round trip on or off (for the tests of the page).
    #[cfg(test)]
    pub(crate) fn set_round(&mut self, on: bool) {
        self.round = on;
    }

    /// The ship chosen: its cargo space goes into the field.
    pub(crate) fn set_ship(&mut self, name: &str, scu: i64) {
        self.ship = Some(name.to_string());
        self.ship_text = name.to_string();
        self.scu_text = scu.to_string();
    }

    pub(crate) fn choose_start(&mut self, ctx: &egui::Context, ident: &str, name: &str) {
        self.failed_starts.retain(|failed| failed != ident);
        self.start = Some((ident.to_string(), name.to_string()));
        self.place_text = name.to_string();
        self.everywhere = false;
        self.chains = None;
        self.chain_job = None;
        if self.book.trips(ident).is_none()
            && let Some(paths) = self.paths.clone()
        {
            let ident_owned = ident.to_string();
            let wanted = ident.to_string();
            self.start_job = Some((
                wanted,
                Job::spawn(ctx, "routes-start", move || {
                    let mut book = Routes::load(&paths);
                    routes::fetch_start(&paths, &mut book, &ident_owned, false, trade::now(), &trade::fetcher())
                }),
            ));
        }
    }

    fn key(&self) -> Option<Key> {
        let (start, _) = self.start.clone()?;
        Some(Key {
            start,
            scu: self.scu(),
            money: self.money(),
            short: self.short,
            stops: self.stops,
            round: self.round,
        })
    }

    fn poll_jobs(&mut self, ui: &Ui) {
        let mut reload = false;
        if let Some((ident, job)) = &self.start_job {
            match job.poll() {
                Polled::Running => {}
                Polled::Done(true) => {
                    self.start_job = None;
                    reload = true;
                }
                Polled::Done(false) | Polled::Lost => {
                    self.failed_starts.push(ident.clone());
                    self.start_job = None;
                    reload = true;
                }
            }
        }
        if let Some((key, job)) = &self.chain_job {
            match job.poll() {
                Polled::Running => {}
                Polled::Done(found) => {
                    self.chains = Some((key.clone(), found));
                    self.chain_job = None;
                    reload = true;
                }
                Polled::Lost => self.chain_job = None,
            }
        }
        if let Some((job, _)) = &self.all_job
            && !matches!(job.poll(), Polled::Running)
        {
            self.all_job = None;
            reload = true;
        }
        if let Some(job) = &self.ships_job
            && !matches!(job.poll(), Polled::Running)
        {
            self.ships_job = None;
            reload = true;
        }
        if reload {
            self.reload();
            ui.ctx().request_repaint();
        }
    }

    /// The chains from the start: the finished ones, and a job for the asked ones.
    fn ensure_chains(&mut self, ctx: &egui::Context) {
        let Some(key) = self.key() else { return };
        if self.chains.as_ref().is_some_and(|(done, _)| *done == key) || self.chain_job.is_some() {
            return;
        }
        // Without the trips of the start there is nothing to chain yet.
        if self.book.trips(&key.start).is_none() {
            return;
        }
        let Some(paths) = self.paths.clone() else { return };
        let asked = key.clone();
        self.chain_job = Some((
            key,
            Job::spawn(ctx, "routes-chain", move || {
                let mut book = Routes::load(&paths);
                let fetch = trade::fetcher();
                let now = trade::now();
                routes::chain(
                    &mut book,
                    &asked.start,
                    asked.scu,
                    asked.money as f64,
                    ChainOptions {
                        short: asked.short,
                        most: CHAINS_SHOWN,
                        stops: asked.stops,
                        round_trip: asked.round,
                    },
                    &mut |book, place| routes::fetch_start(&paths, book, place, false, now, &fetch),
                )
            }),
        ));
    }
}

// ------------------------------------------------------------ the page

/// The page.
pub fn show_page(ui: &mut Ui, scheme: &Scheme, state: &mut RoutesState) {
    state.follow(ui);
    state.poll_jobs(ui);
    if !state.ships_asked && state.market.is_empty() {
        state.ships_asked = true;
        if let Some(paths) = state.paths.clone() {
            let ctx = ui.ctx().clone();
            state.ships_job = Some(Job::spawn(&ctx, "routes-ships", move || {
                ship_market::update(&paths, trade::now(), &trade::fetcher())
            }));
        }
    }
    let pass = ui.ctx().cumulative_pass_nr();
    let opened = state.shown_pass.is_none_or(|last| pass > last + 1);
    state.shown_pass = Some(pass);
    trade::quiet(ui, scheme, &t("s_hd_lead_rt"));
    ui.add_space(4.0);
    inputs(ui, scheme, state, opened);
    ui.add_space(GAP);
    toggles(ui, scheme, state);
    ui.add_space(GAP);
    results(ui, scheme, state);
}

/// The fields. While no start is chosen the field for it wears a ring and a line says to choose it first;
/// the field takes the keyboard focus when the page opens.
fn inputs(ui: &mut Ui, scheme: &Scheme, state: &mut RoutesState, opened: bool) {
    let posts = routes::trade_posts(&state.selling);
    let systems = state.systems(&posts);
    ui.label(RichText::new(t("s_rt_wo")).strong());
    // The system narrows the list of places.
    if !systems.is_empty() {
        let mut words = vec![t("s_rt_alle_systeme")];
        words.extend(systems.iter().map(|(name, count)| format!("{name} ({count})")));
        let refs: Vec<&str> = words.iter().map(String::as_str).collect();
        let mut chosen = [state.system.min(words.len() - 1)];
        if widgets::filter_bar(ui, "routen-system", &[&refs], &mut chosen) {
            state.system = chosen[0];
            state.place_text.clear();
        }
    }
    let wanted_system = state.system.checked_sub(1).and_then(|i| systems.get(i)).map(|(name, _)| name.clone());
    let mut labels: Vec<String> = Vec::new();
    let mut idents: Vec<(String, String)> = Vec::new();
    for (ident, name) in &posts {
        let spot = state.selling.terminals.get(ident);
        let system = spot.map(|s| s.system.clone()).unwrap_or_default();
        if wanted_system.as_ref().is_some_and(|w| *w != system) {
            continue;
        }
        let place = spot.map(|s| s.place.clone()).unwrap_or_default();
        let mut label = name.clone();
        for part in [&place, &system] {
            if !part.is_empty() && *part != *name {
                label = format!("{label} · {part}");
            }
        }
        labels.push(label);
        idents.push((ident.clone(), name.clone()));
    }
    let more = |n: usize| tf("s_mlg_mehr", &[("n", Arg::from(n))]);
    let hint = t("s_hd_wo_ph");
    let width = ui.available_width() - layout::SAFETY;
    let mut text = std::mem::take(&mut state.place_text);
    let waiting = state.start.is_none();
    let field = widgets::SuggestField::new("handel-start", &hint, scheme, &more)
        .width(width)
        .attention(waiting)
        .show(ui, &mut text, &labels);
    let picked = field.picked;
    if waiting {
        if opened {
            field.response.request_focus();
        }
        trade::quiet(ui, scheme, &t("s_hd_rt_start_zuerst"));
    }
    state.place_text = text;
    if let Some(index) = picked
        && let Some((ident, name)) = idents.get(index).cloned()
    {
        let ctx = ui.ctx().clone();
        state.choose_start(&ctx, &ident, &name);
    }
    ui.add_space(GAP);
    layout::columns_or_stack(ui, 240.0, 2, |ui, i| {
        if i == 0 {
            ui.label(t("s_rt_scu"));
            ui.add(
                widgets::text_field(&mut state.scu_text, &t("s_pl_scu"), scheme)
                    .desired_width(ui.available_width() - layout::SAFETY),
            );
        } else {
            ui.label(t("s_rt_geld"));
            ui.add(
                widgets::text_field(&mut state.money_text, &t("s_pl_geld"), scheme)
                    .desired_width(ui.available_width() - layout::SAFETY),
            );
        }
    });
    ui.add_space(GAP);
    ship_picker(ui, scheme, state);
}

fn ship_picker(ui: &mut Ui, scheme: &Scheme, state: &mut RoutesState) {
    ui.label(RichText::new(t("s_hd_schiff")).strong());
    let cargo = state.market.with_cargo();
    if cargo.is_empty() {
        trade::quiet(ui, scheme, &t("s_hd_keine_schiffe"));
        return;
    }
    let mut makers: Vec<(String, usize)> = Vec::new();
    for ship in &cargo {
        if ship.maker.is_empty() {
            continue;
        }
        match makers.iter_mut().find(|(name, _)| *name == ship.maker) {
            Some(entry) => entry.1 += 1,
            None => makers.push((ship.maker.clone(), 1)),
        }
    }
    makers.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| lower(&a.0).cmp(&lower(&b.0))));
    if makers.len() > 1 {
        let mut words = vec![t("s_rt_alle_werften")];
        words.extend(makers.iter().map(|(name, count)| format!("{name} ({count})")));
        let refs: Vec<&str> = words.iter().map(String::as_str).collect();
        let mut chosen = [state.maker.min(words.len() - 1)];
        if widgets::filter_bar(ui, "routen-werft", &[&refs], &mut chosen) {
            state.maker = chosen[0];
            state.ship = None;
            state.ship_text.clear();
        }
    }
    let wanted_maker = state.maker.checked_sub(1).and_then(|i| makers.get(i)).map(|(name, _)| name.clone());
    let names: Vec<(String, i64)> = cargo
        .iter()
        .filter(|s| wanted_maker.as_ref().is_none_or(|m| *m == s.maker))
        .map(|s| (s.name.clone(), s.scu))
        .collect();
    let labels: Vec<String> = names.iter().map(|(name, _)| name.clone()).collect();
    let more = |n: usize| tf("s_mlg_mehr", &[("n", Arg::from(n))]);
    let hint = t("s_rt_schiff_platz");
    let width = ui.available_width() - layout::SAFETY;
    let mut text = std::mem::take(&mut state.ship_text);
    let picked = widgets::SuggestField::new("handel-schiff", &hint, scheme, &more)
        .width(width)
        .show(ui, &mut text, &labels)
        .picked;
    state.ship_text = text;
    if let Some(index) = picked
        && let Some((name, scu)) = names.get(index)
    {
        state.set_ship(name, *scu);
    }
    if let Some(ship) = state.ship.clone() {
        let lang = language::current();
        for (key, offers) in [("s_rt_kaufen", state.market.buy_at(&ship)), ("s_rt_mieten", state.market.rent_at(&ship))]
        {
            if let Some(first) = offers.first() {
                let place: Vec<&str> =
                    [first.spot.as_str(), first.system.as_str()].into_iter().filter(|s| !s.is_empty()).collect();
                trade::quiet(
                    ui,
                    scheme,
                    &format!("{}  {}  ·  {}", t(key), trade::auec(lang, first.price), place.join(" · ")),
                );
            }
        }
    }
}

fn toggles(ui: &mut Ui, scheme: &Scheme, state: &mut RoutesState) {
    let mut changed = false;
    let mut short = usize::from(state.short);
    let words = [t("s_rt_nach_gewinn"), t("s_rt_nach_strecke")];
    changed |= trade::toggle_pair(ui, scheme, [&words[0], &words[1]], &mut short);
    state.short = short == 1;
    ui.add_space(2.0);
    ui.horizontal_wrapped(|row| {
        for stops in 2..=routes::MAX_STOPS {
            let kind = if state.stops == stops { ButtonKind::Strong } else { ButtonKind::Normal };
            if row.add(widgets::Button::new(&tf("s_rt_stopps", &[("0", Arg::from(stops))]), kind, scheme)).clicked() {
                state.stops = stops;
                changed = true;
            }
        }
    });
    ui.add_space(2.0);
    let mut round = usize::from(state.round);
    let words = [t("s_rt_offen"), t("s_rt_rund")];
    changed |= trade::toggle_pair(ui, scheme, [&words[0], &words[1]], &mut round);
    state.round = round == 1;
    if changed {
        state.chains = None;
    }
}

fn results(ui: &mut Ui, scheme: &Scheme, state: &mut RoutesState) {
    let lang = language::current();
    everywhere_bar(ui, scheme, state);
    ui.add_space(GAP);
    let row_height = ui.text_style_height(&egui::TextStyle::Body) + 10.0;
    let room = layout::list_room(ui, row_height);
    let area = egui::ScrollArea::vertical()
        .id_salt(LIST_ID)
        .max_height(room)
        .min_scrolled_height(room)
        .auto_shrink([false, true]);
    let shown = widgets::scroll_area(ui, scheme, area, |ui| {
        ui.set_max_width(ui.available_width() - layout::SAFETY);
        if state.all_job.is_some() {
            widgets::loading(ui, scheme, &t("s_rt_rechnet"));
        } else if state.everywhere {
            everywhere_list(ui, scheme, state, lang);
        } else if state.start.is_some() {
            from_start(ui, scheme, state, lang);
        }
    });
    state.list_overflow = trade::overflow_of(&shown);
}

/// The button for the search of all posts, its progress and the number of posts known.
fn everywhere_bar(ui: &mut Ui, scheme: &Scheme, state: &mut RoutesState) {
    let posts = routes::trade_posts(&state.selling);
    let running = state.all_job.is_some();
    ui.horizontal_wrapped(|row| {
        let text = if running { t("s_rt_ueberall_laeuft") } else { t("s_rt_ueberall_suchen") };
        let reason = if running {
            Some(t("s_hd_alle_laeuft"))
        } else if posts.is_empty() || state.paths.is_none() {
            Some(t("s_hd_alle_ohne"))
        } else {
            None
        };
        let button = widgets::Button::new(&text, ButtonKind::Normal, scheme).icon("compass").blocked(reason.as_deref());
        if row.add(button).clicked()
            && let Some(paths) = state.paths.clone()
        {
            let ctx = row.ctx().clone();
            let wake = ctx.clone();
            let progress = Arc::new(AtomicUsize::new(0));
            let counter = progress.clone();
            let posts = posts.clone();
            state.everywhere = true;
            state.start = None;
            state.all_job = Some((
                Job::spawn(&ctx, "routes-all", move || {
                    let mut book = Routes::load(&paths);
                    let ctx_wake = wake;
                    routes::fetch_all(
                        &paths,
                        &mut book,
                        &posts,
                        trade::now(),
                        &trade::fetcher(),
                        &mut |done, _| {
                            counter.store(done, Ordering::Relaxed);
                            ctx_wake.request_repaint();
                        },
                        &|| false,
                    )
                }),
                progress,
            ));
        }
        let known = state.book.known_starts(&posts);
        let done = state.all_job.as_ref().map_or(known, |(_, progress)| progress.load(Ordering::Relaxed));
        if !posts.is_empty() {
            trade::quiet(
                row,
                scheme,
                &tf("s_rt_ueberall_stand", &[("0", Arg::from(done)), ("1", Arg::from(posts.len()))]),
            );
        }
    });
}

fn from_start(ui: &mut Ui, scheme: &Scheme, state: &mut RoutesState, lang: Lang) {
    let Some((ident, name)) = state.start.clone() else { return };
    let now = trade::now();
    if state.start_job.is_some() {
        widgets::loading(ui, scheme, &t("s_rt_rechnet"));
        return;
    }
    trade::two_sided(
        ui,
        |left| {
            left.label(RichText::new(&name).strong());
        },
        |side| {
            if let Some(age) = state.book.age(&ident, now) {
                trade::quiet(side, scheme, &tf("s_vk_stand", &[("alter", Arg::from(trade::age_text(Some(age))))]));
            }
        },
    );
    ui.add_space(4.0);
    let single = routes::single_trips(&state.book, &ident, state.scu(), state.money() as f64, SINGLE_ASKED);
    if single.is_empty() {
        // Trips that could not be fetched are no proof that nothing pays.
        let failed = state.book.trips(&ident).is_none() && state.failed_starts.contains(&ident);
        trade::quiet(ui, scheme, &t(if failed { "s_vk_fehler" } else { "s_hd_nichts" }));
        return;
    }
    ui.label(RichText::new(t("s_rt_einzeln")).strong());
    for (n, leg) in single.iter().take(SINGLE_SHOWN).enumerate() {
        leg_row(ui, scheme, lang, leg, n == 0, Some(&name));
    }
    ui.add_space(GAP);
    let title =
        if state.round { t("s_hd_rundreise_titel") } else { tf("s_rt_ketten", &[("0", Arg::from(state.stops))]) };
    ui.label(RichText::new(title).strong());
    let ctx = ui.ctx().clone();
    state.ensure_chains(&ctx);
    if state.chain_job.is_some() {
        widgets::loading(ui, scheme, &t("s_rt_rechnet"));
        return;
    }
    let current = state.key();
    match state.chains.as_ref().filter(|(key, _)| Some(key) == current.as_ref()) {
        Some((_, found)) if !found.is_empty() => {
            for (n, (profit, way)) in found.iter().enumerate() {
                chain_box(ui, scheme, lang, *profit, way, &name, n == 0, state.round);
                ui.add_space(4.0);
            }
        }
        Some(_) => trade::quiet(ui, scheme, &t("s_hd_keine_kette")),
        None => {}
    }
}

fn everywhere_list(ui: &mut Ui, scheme: &Scheme, state: &mut RoutesState, lang: Lang) {
    ui.label(RichText::new(t("s_rt_ueberall_titel")).strong());
    let posts = routes::trade_posts(&state.selling);
    let options =
        ChainOptions { short: state.short, most: CHAINS_ANYWHERE, stops: state.stops, round_trip: state.round };
    let (scu, money) = (state.scu(), state.money());
    let key: EverywhereKey = (scu, money, state.short, state.stops, state.round, state.book_version);
    if state.everywhere_cache.as_ref().is_none_or(|(known, _)| *known != key) {
        let found = routes::best_chains_anywhere(&mut state.book, &posts, scu, money as f64, options);
        state.everywhere_cache = Some((key, found));
    }
    let found = state.everywhere_cache.as_ref().map_or(&[][..], |(_, found)| &found[..]);
    if found.is_empty() {
        trade::quiet(ui, scheme, &t("s_hd_keine_kette"));
        return;
    }
    for (profit, start, way) in found {
        chain_box(ui, scheme, lang, *profit, way, start, false, false);
        ui.add_space(4.0);
    }
}

/// One single trip: profit, amount, good, where to sell; below the outlay, the stock and what limits it.
fn leg_row(ui: &mut Ui, scheme: &Scheme, lang: Lang, leg: &Leg, first: bool, start: Option<&str>) {
    widgets::list_frame(ui, scheme, |ui| {
        ui.set_min_width(ui.available_width() - layout::SAFETY);
        ui.horizontal_wrapped(|row| {
            let tone = if first { scheme.accent } else { scheme.fg };
            row.label(RichText::new(trade::auec(lang, leg.profit)).strong().color(color(tone)));
            row.label(tf(
                "s_rt_scu_menge",
                &[("0", Arg::from(formats::amount(lang, leg.amount as f64, "").trim_end().to_string()))],
            ));
            row.label(RichText::new(&leg.trip.good).strong());
            if let Some(start) = start {
                row.label(RichText::new(tf("s_rt_ab", &[("0", Arg::from(start))])).color(color(scheme.sub)));
            }
            let target = if leg.trip.dest_name.is_empty() { "?" } else { leg.trip.dest_name.as_str() };
            row.label(tf("s_rt_nach", &[("0", Arg::from(target))]));
            if leg.trip.distance > 0.0 {
                row.label(
                    RichText::new(tf("s_rt_strecke", &[("0", Arg::from(leg.trip.distance as i64))]))
                        .color(color(scheme.sub)),
                );
            }
        });
        if leg.trip.buy != 0.0 {
            ui.horizontal_wrapped(|row| {
                let outlay = leg.trip.buy * leg.amount as f64;
                row.label(
                    RichText::new(tf("s_rt_einsatz", &[("0", Arg::from(trade::plain(lang, outlay)))]))
                        .small()
                        .color(color(scheme.sub)),
                );
                if leg.trip.stock > 0 {
                    row.label(
                        RichText::new(tf("s_rt_vorrat", &[("0", Arg::from(leg.trip.stock))]))
                            .small()
                            .color(color(scheme.sub)),
                    );
                }
                if !leg.limit.is_empty() {
                    row.label(RichText::new(t(leg.limit)).small().color(color(scheme.yellow)));
                }
            });
        }
    });
}

#[allow(clippy::too_many_arguments)]
fn chain_box(
    ui: &mut Ui,
    scheme: &Scheme,
    lang: Lang,
    profit: f64,
    way: &[Leg],
    start_name: &str,
    first: bool,
    round: bool,
) {
    widgets::list_frame(ui, scheme, |ui| {
        ui.set_min_width(ui.available_width() - layout::SAFETY);
        let distance: f64 = way.iter().map(|leg| leg.trip.distance).sum();
        trade::two_sided(
            ui,
            |left| {
                let tone = if first { scheme.accent } else { scheme.fg };
                left.label(
                    RichText::new(tf("s_rt_kette_gewinn", &[("0", Arg::from(trade::plain(lang, profit)))]))
                        .strong()
                        .color(color(tone)),
                );
            },
            |side| {
                if distance > 0.0 {
                    trade::quiet(side, scheme, &tf("s_rt_strecke", &[("0", Arg::from(distance as i64))]));
                }
            },
        );
        if let Some(first_leg) = way.first() {
            let outlay = first_leg.trip.buy * first_leg.amount as f64;
            if outlay != 0.0 {
                trade::quiet(ui, scheme, &tf("s_rt_kette_einsatz", &[("0", Arg::from(trade::plain(lang, outlay)))]));
            }
        }
        let mut stations = vec![start_name.to_string()];
        stations.extend(way.iter().map(|leg| leg.trip.dest_name.clone()));
        ui.add(egui::Label::new(RichText::new(stations.join(" → ")).color(color(scheme.sub))).wrap());
        let mut from = start_name.to_string();
        for (n, leg) in way.iter().enumerate() {
            let mut to = if leg.trip.dest_name.is_empty() { "?".to_string() } else { leg.trip.dest_name.clone() };
            if round && n + 1 == way.len() {
                to = tf("s_rt_zurueck", &[("0", Arg::from(to))]);
            }
            let mut text = tf(
                "s_hd_schritt",
                &[
                    ("n", Arg::from(n + 1)),
                    ("von", Arg::from(from.as_str())),
                    ("menge", Arg::from(leg.amount)),
                    ("ware", Arg::from(leg.trip.good.as_str())),
                    ("nach", Arg::from(to)),
                ],
            );
            let outlay = leg.trip.buy * leg.amount as f64;
            if outlay != 0.0 {
                text = format!("{text}  ·  {}", tf("s_rt_schritt_ek", &[("0", Arg::from(trade::auec(lang, outlay)))]));
            }
            ui.add(egui::Label::new(text).wrap());
            from = if leg.trip.dest_name.is_empty() { "?".to_string() } else { leg.trip.dest_name.clone() };
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Rect, Shape, vec2};
    use versekit_core::language::{Choice, set_language};

    /// One page of the routes in its own context, drawn frame by frame.
    struct Rig {
        ctx: egui::Context,
        state: RoutesState,
    }

    /// What one frame drew: every text with its place and the rings of two points in the accent.
    struct Frame {
        texts: Vec<(String, Rect)>,
        rings: Vec<Rect>,
    }

    impl Frame {
        fn shows(&self, text: &str) -> bool {
            self.texts.iter().any(|(shown, _)| shown == text)
        }

        /// The ring that runs around the text, if there is one.
        fn ring_around(&self, text: &str) -> Option<Rect> {
            let hint = self.texts.iter().find(|(shown, _)| shown == text)?.1;
            self.rings.iter().copied().find(|ring| ring.contains_rect(hint))
        }
    }

    impl Rig {
        fn new() -> Rig {
            let ctx = egui::Context::default();
            crate::install(&ctx);
            Rig { ctx, state: RoutesState::default() }
        }

        fn run(&mut self, events: Vec<egui::Event>, draw: bool) -> Frame {
            let scheme = versekit_core::theme::current();
            let state = &mut self.state;
            let mut output = self.ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(1280.0, 820.0))),
                    events,
                    ..Default::default()
                },
                |ui| {
                    if draw {
                        crate::layout::page(ui, scheme, "routen-test", |ui| show_page(ui, scheme, state));
                    }
                },
            );
            output.textures_delta.clear();
            let accent = color(scheme.accent);
            let mut frame = Frame { texts: Vec::new(), rings: Vec::new() };
            fn collect(shape: &Shape, accent: egui::Color32, frame: &mut Frame) {
                match shape {
                    Shape::Text(text) => frame
                        .texts
                        .push((text.galley.text().to_string(), text.galley.rect.translate(text.pos.to_vec2()))),
                    Shape::Rect(rect) if rect.stroke.width == 2.0 && rect.stroke.color == accent => {
                        frame.rings.push(rect.rect)
                    }
                    Shape::Vec(shapes) => shapes.iter().for_each(|s| collect(s, accent, frame)),
                    _ => {}
                }
            }
            output.shapes.iter().for_each(|clipped| collect(&clipped.shape, accent, &mut frame));
            frame
        }

        /// A few frames of the page.
        fn settle(&mut self) -> Frame {
            let mut last = self.run(Vec::new(), true);
            for _ in 0..3 {
                last = self.run(Vec::new(), true);
            }
            last
        }

        fn type_text(&mut self, text: &str) {
            self.run(vec![egui::Event::Text(text.to_string())], true);
            self.run(Vec::new(), true);
        }
    }

    /// Without a start the field for it wears a ring and a line says to choose it first; with a start both are gone.
    #[test]
    fn the_field_for_the_start_waits_with_a_ring_and_a_line_until_a_start_is_chosen() {
        let _language = crate::language_lock();
        set_language(Choice::De);
        let (hint, line) = (t("s_hd_wo_ph"), t("s_hd_rt_start_zuerst"));
        let mut rig = Rig::new();
        let waiting = rig.settle();
        assert!(waiting.shows(&line), "{:?}", waiting.texts);
        assert!(waiting.ring_around(&hint).is_some(), "{:?}", waiting.rings);
        // The list below stays empty: no second sentence about the same thing.
        assert!(!waiting.texts.iter().any(|(shown, _)| shown.starts_with("Gib oben ein")), "{:?}", waiting.texts);
        let ctx = rig.ctx.clone();
        rig.state.choose_start(&ctx, "1", "Admin Olisar");
        let chosen = rig.settle();
        assert!(!chosen.shows(&line), "{:?}", chosen.texts);
        assert!(chosen.rings.is_empty(), "{:?}", chosen.rings);
    }

    /// The field takes the keyboard focus when the page opens, once: not again while the page stays open,
    /// but again when the page is opened anew.
    #[test]
    fn the_field_for_the_start_takes_the_focus_once_when_the_page_opens() {
        let _language = crate::language_lock();
        set_language(Choice::De);
        let mut rig = Rig::new();
        rig.settle();
        rig.type_text("Lor");
        assert_eq!(rig.state.place_text, "Lor", "typing lands in the field right away");
        // The player goes elsewhere in the page: the page does not pull the focus back.
        rig.ctx.memory_mut(|m| {
            if let Some(id) = m.focused() {
                m.surrender_focus(id);
            }
        });
        rig.settle();
        rig.type_text("x");
        assert_eq!(rig.state.place_text, "Lor", "the focus stays where the player put it");
        // Another page for a while, then the page again.
        for _ in 0..3 {
            rig.run(Vec::new(), false);
        }
        rig.settle();
        rig.type_text("y");
        assert_eq!(rig.state.place_text, "Lory", "opened anew: the field has the focus again");
    }
}
