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

//! The page "Shops" (`pages._shops` of 4.12): where a finished part stands
//! on the shelf and what it costs, and the ships to buy or rent.
//!
//! The list is the catalog of UEX (about 1.500 parts that somebody really
//! sells, in about 40 groups) plus the ships; it is built in the background
//! on first use. Five menus narrow it, each showing only what fits the ones
//! before: area, group, size, class, grade. A place narrows it too. Choosing
//! a part fetches its shops; the cheapest stands on top.
//!
//! New in v5: a part goes into the shopping list with how many of it
//! (`basket.rs`), and from a part the way leads back to the list of results
//! with the search and the menus as they were (4.12 emptied the search).

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::SystemTime;

use egui::{RichText, Ui};
use versekit_core::basket;
use versekit_core::language::{self, Arg, Lang, t, tf};
use versekit_core::paths::Paths;
use versekit_core::ship_market::{self, Market};
use versekit_core::shops::{self, Catalog, Shops};
use versekit_core::stock_data::lower;
use versekit_core::theme::Scheme;
use versekit_gamedata::craft_book::CraftBook;
use versekit_gamedata::values_store;

use crate::color;
use crate::hangar_page::HangarState;
use crate::layout::{self, GAP};
use crate::trade::{self, Job, Polled};
use crate::widgets::{self, ButtonKind};

/// The area of the ships, which are no parts of the catalog.
const AREA_SHIPS: &str = "__schiffe";
const SHIPYARD_PREFIX: &str = "werft:";
const SHIP_PREFIX: &str = "schiff:";
/// Rows of one group in the overview of several groups, and the most of a flat list.
const PER_GROUP: usize = 12;
const EMERGENCY_CAP: usize = 400;
const LOOK_SECONDS: f64 = 1.0;
/// How long the page waits before it looks at the catalog again, after a build or a failed one.
const CATALOG_LOOK_SECONDS: f64 = 30.0 * 60.0;
const LIST_ID: &str = "handel-shops-liste";

/// The five menus in their order.
const FIELDS: [Field; 5] = [Field::Area, Field::Group, Field::Size, Field::Class, Field::Grade];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Field {
    Area,
    Group,
    Size,
    Class,
    Grade,
}

/// A part or a ship of the list.
#[derive(Clone, Debug)]
struct Entry {
    name: String,
    code: String,
    area: String,
    group: String,
    maker: String,
    size: String,
    class: String,
    grade: String,
    places: Vec<String>,
    /// What the search looks through: name, maker, area and group, in lower case, raw and written out.
    hay: String,
}

/// The parts by size, then by name (`size_order`); the size of UEX is text and read as a number.
fn in_size_order(entries: &mut [Entry]) {
    entries.sort_by_cached_key(|e| versekit_core::size_order::key(versekit_core::size_order::parse(&e.size), &e.name));
}

impl Entry {
    fn search_text(&self) -> String {
        shops::search_text(
            &self.name,
            &self.maker,
            &label_of(Field::Area, &self.area),
            &label_of(Field::Group, &self.group),
            &self.area,
            &self.group,
        )
    }

    fn is_ship(&self) -> bool {
        self.area == AREA_SHIPS
    }

    fn value(&self, field: Field) -> &str {
        match field {
            Field::Area => &self.area,
            Field::Group => &self.group,
            Field::Size => &self.size,
            Field::Class => &self.class,
            Field::Grade => &self.grade,
        }
    }
}

/// The class names UEX writes, with the text key of their translation.
fn class_key(class: &str) -> Option<&'static str> {
    Some(match class {
        "Civilian" => "s_ld_kl_civilian",
        "Military" => "s_ld_kl_military",
        "Industrial" => "s_ld_kl_industrial",
        "Stealth" => "s_ld_kl_stealth",
        "Competition" => "s_ld_kl_competition",
        "Medical" => "s_ld_kl_medical",
        "Mining" => "s_ld_kl_mining",
        "Salvage and Repair" => "s_ld_kl_salvage",
        _ => return None,
    })
}

/// The part or ship chosen.
#[derive(Clone, Debug)]
struct Chosen {
    code: String,
    name: String,
}

/// What the page holds.
pub struct ShopsState {
    paths: Option<Arc<Paths>>,
    catalog: Catalog,
    shops: Shops,
    market: Market,
    entries: Vec<Entry>,
    seen: [Option<SystemTime>; 3],
    looked_at: f64,
    search: String,
    place_text: String,
    place: Option<String>,
    wanted: [String; 5],
    chosen: Option<Chosen>,
    catalog_job: Option<(Job<bool>, Arc<AtomicUsize>, Arc<AtomicUsize>)>,
    part_job: Option<(String, Job<bool>)>,
    failed: Vec<String>,
    /// The part chosen while another request was running, to be asked for next: (code, name).
    queued_part: Option<(String, String)>,
    /// When the catalog is looked at again (page clock): built when missing, rebuilt when stale.
    catalog_retry: f64,
    /// The language the search texts of the entries are written in.
    hay_lang: Lang,
    /// How far the list sticks out at the right (the edge check reads it).
    list_overflow: f32,
    /// How many of the open part go into the shopping list.
    amount: i64,
    /// What the last "into the shopping list" said: the line, and whether it failed.
    added: Option<(String, bool)>,
    /// What the last "plan in hangar" of the open ship said: the line, and whether it failed.
    planned: Option<(String, bool)>,
    /// The blueprints with an entity code, the list while there is no catalog and none is fetched;
    /// `None` until read.
    fallback: Option<Vec<Entry>>,
    fallback_job: Option<Job<Vec<Entry>>>,
    /// The shop whose whole offer is open, chosen in the list of a place or by its name in the field of the place;
    /// held by name and place, so that a catalog read again finds it at whatever index it has there.
    outlet: Option<shops::Outlet>,
}

/// What the page asks of the window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// Open the section "Planned" of the hangar page.
    ToPlanned,
}

impl Default for ShopsState {
    fn default() -> Self {
        ShopsState {
            paths: None,
            catalog: Catalog::default(),
            shops: Shops::default(),
            market: Market::default(),
            entries: Vec::new(),
            seen: [None, None, None],
            looked_at: f64::NEG_INFINITY,
            search: String::new(),
            place_text: String::new(),
            place: None,
            wanted: Default::default(),
            chosen: None,
            catalog_job: None,
            part_job: None,
            failed: Vec::new(),
            queued_part: None,
            catalog_retry: 0.0,
            hay_lang: language::current(),
            list_overflow: 0.0,
            amount: 1,
            added: None,
            planned: None,
            fallback: None,
            fallback_job: None,
            outlet: None,
        }
    }
}

/// The list without a catalog (`_teile` of `pages._shops`): every blueprint whose product has an entity
/// code, by that code, without area, in the group of its kind (`shops::blueprint_group`).
fn blueprint_entries(book: &CraftBook) -> Vec<Entry> {
    let mut entries: Vec<Entry> = book
        .items
        .iter()
        .filter(|item| !item.entity.is_empty())
        .map(|item| Entry {
            name: item.name.clone(),
            code: item.entity.clone(),
            area: String::new(),
            group: shops::blueprint_group(&item.kind, &item.subtype),
            maker: String::new(),
            size: String::new(),
            class: String::new(),
            grade: String::new(),
            places: Vec::new(),
            hay: String::new(),
        })
        .collect();
    in_size_order(&mut entries);
    entries
}

fn modified(file: &str) -> Option<SystemTime> {
    std::fs::metadata(file).and_then(|m| m.modified()).ok()
}

impl ShopsState {
    /// With own data: the catalog, the shop prices and the ships of the data folder.
    pub fn with_files(paths: &Paths) -> ShopsState {
        let mut state = ShopsState { paths: Some(trade::job_paths(paths)), ..ShopsState::default() };
        state.reload();
        state
    }

    fn files(&self) -> Option<[String; 3]> {
        let paths = self.paths.as_ref()?;
        Some([paths.app_file(shops::CATALOG_CACHE), paths.app_file(shops::CACHE), paths.app_file(ship_market::CACHE)])
    }

    fn reload(&mut self) {
        let Some(paths) = self.paths.clone() else { return };
        if let Some(files) = self.files() {
            self.seen = [modified(&files[0]), modified(&files[1]), modified(&files[2])];
        }
        self.catalog = Catalog::load(&paths);
        self.shops = Shops::load(&paths);
        self.market = Market::load(&paths);
        self.rebuild();
    }

    /// The list: the parts of the catalog, and the ships once the catalog is there. Without a catalog and
    /// while none is fetched, the blueprints stand in; while it is fetched the list stays empty.
    fn rebuild(&mut self) {
        self.entries.clear();
        if !self.catalog.ready() {
            if self.catalog_job.is_none()
                && let Some(fallback) = &self.fallback
            {
                self.entries = fallback.clone();
                self.refresh_hay();
            }
            return;
        }
        for item in self.catalog.items() {
            if item.name.is_empty() || item.code.is_empty() {
                continue;
            }
            self.entries.push(Entry {
                name: item.name,
                code: item.code,
                area: item.section,
                group: item.group,
                maker: item.maker,
                size: item.size,
                class: item.class,
                grade: item.grade,
                places: item.places,
                hay: String::new(),
            });
        }
        in_size_order(&mut self.entries);
        for ship in self.market.catalog() {
            self.entries.push(Entry {
                name: ship.name.clone(),
                code: format!("{SHIP_PREFIX}{}", ship.name),
                area: AREA_SHIPS.to_string(),
                group: format!("{SHIPYARD_PREFIX}{}", if ship.maker.is_empty() { "?" } else { &ship.maker }),
                maker: String::new(),
                size: String::new(),
                class: String::new(),
                grade: String::new(),
                places: Vec::new(),
                hay: String::new(),
            });
        }
        self.refresh_hay();
    }

    /// Writes the search text of every entry again, in the language in force.
    fn refresh_hay(&mut self) {
        self.hay_lang = language::current();
        for entry in &mut self.entries {
            entry.hay = entry.search_text();
        }
    }

    fn follow(&mut self, ui: &Ui) {
        if self.hay_lang != language::current() {
            self.refresh_hay();
        }
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

    /// Is something typed or chosen that "Reset" would undo?
    pub fn is_narrowed(&self) -> bool {
        !self.search.is_empty()
            || !self.place_text.is_empty()
            || self.place.is_some()
            || self.chosen.is_some()
            || self.outlet.is_some()
            || self.wanted.iter().any(|w| !w.is_empty())
    }

    /// Back to the start; the catalog stays.
    pub fn reset(&mut self) {
        self.search.clear();
        self.place_text.clear();
        self.place = None;
        self.chosen = None;
        self.outlet = None;
        self.wanted = Default::default();
    }

    /// What the text of the field of the place says: empty is no place and no shop; exactly a place is that place;
    /// exactly the name of one shop opens that shop. A shop opened from the list stays while the text stays
    /// (`changed` is false); a text that names nothing known leaves the place as it was.
    fn take_place_text(&mut self, changed: bool) {
        let typed = lower(self.place_text.trim());
        if typed.is_empty() {
            self.place = None;
            self.outlet = None;
            return;
        }
        if let Some(found) = self.catalog.places().into_iter().find(|p| lower(p) == typed) {
            if changed || self.place.as_deref() != Some(found.as_str()) {
                self.outlet = None;
            }
            self.place = Some(found);
            return;
        }
        if !changed {
            return;
        }
        match self.catalog.outlet_named(&typed) {
            Some(at) => self.open_outlet(at),
            // A text that names nothing known closes a shop that was open; the place stays as it was.
            None => self.outlet = None,
        }
    }

    /// Does the text of the field name a place, or the shop that is open?
    fn place_text_known(&self) -> bool {
        let typed = lower(self.place_text.trim());
        typed.is_empty()
            || self.catalog.places().iter().any(|p| lower(p) == typed)
            || self.outlet_index().is_some_and(|at| lower(&self.catalog.outlet_label(at)) == typed)
    }

    /// Opens the whole offer of the shop `at` of the catalog, at its place.
    fn open_outlet(&mut self, at: usize) {
        let Some(outlet) = self.catalog.outlets.get(at).cloned() else { return };
        self.place = Some(outlet.place.clone());
        self.outlet = Some(outlet);
        self.chosen = None;
        self.normalize();
    }

    /// The index of the open shop in the catalog as it is now; `None` when none is open or the catalog lost it.
    fn outlet_index(&self) -> Option<usize> {
        let outlet = self.outlet.as_ref()?;
        self.catalog.outlets.iter().position(|o| o == outlet)
    }

    /// The entries the open shop sells, each with its price there, in the order of the list; those the search
    /// and the menus leave out are left out here too.
    fn outlet_offers(&self, at: usize) -> Vec<(usize, f64)> {
        let prices: std::collections::HashMap<String, f64> = self.catalog.offers_of(at).into_iter().collect();
        self.entries
            .iter()
            .enumerate()
            .filter(|(_, e)| self.fits(e, None) && shops::search_finds(&e.hay, &self.search))
            .filter_map(|(i, e)| prices.get(&e.code).map(|price| (i, *price)))
            .collect()
    }

    fn at_place(&self, entry: &Entry) -> bool {
        self.place.as_ref().is_none_or(|place| entry.places.iter().any(|p| p == place))
    }

    /// Does the entry fit the place and the chosen menus before `upto` (all five for `None`)?
    fn fits(&self, entry: &Entry, upto: Option<usize>) -> bool {
        if !self.at_place(entry) {
            return false;
        }
        FIELDS.iter().enumerate().take(upto.unwrap_or(FIELDS.len())).all(|(i, field)| {
            let wanted = &self.wanted[i];
            wanted.is_empty() || entry.value(*field) == wanted
        })
    }

    /// The values of a menu with their counts: what fits the place and the menus before it.
    fn menu(&self, index: usize) -> Vec<(String, usize)> {
        let field = FIELDS[index];
        let mut counts: Vec<(String, usize)> = Vec::new();
        for entry in self.entries.iter().filter(|e| self.fits(e, Some(index))) {
            let value = entry.value(field);
            if value.trim().is_empty() {
                continue;
            }
            match counts.iter_mut().find(|(name, _)| name == value) {
                Some(slot) => slot.1 += 1,
                None => counts.push((value.to_string(), 1)),
            }
        }
        if field == Field::Size {
            let number = |text: &str| text.trim().parse::<f64>().map_or((1, 0.0), |n| (0, n));
            counts.sort_by(|a, b| {
                let (na, nb) = (number(&a.0), number(&b.0));
                na.0.cmp(&nb.0).then(na.1.total_cmp(&nb.1))
            });
        } else {
            counts.sort_by(|a, b| {
                b.1.cmp(&a.1).then_with(|| lower(&label_of(field, &a.0)).cmp(&lower(&label_of(field, &b.0))))
            });
        }
        counts
    }

    /// A chosen value that no longer fits drops out, from front to back.
    fn normalize(&mut self) {
        for index in 0..FIELDS.len() {
            if self.wanted[index].is_empty() {
                continue;
            }
            let wanted = self.wanted[index].clone();
            if !self.menu(index).iter().any(|(value, _)| *value == wanted) {
                self.wanted[index].clear();
            }
        }
    }

    /// The entries that fit everything and the search text.
    fn visible(&self) -> Vec<usize> {
        self.entries
            .iter()
            .enumerate()
            .filter(|(_, e)| self.fits(e, None) && shops::search_finds(&e.hay, &self.search))
            .map(|(i, _)| i)
            .collect()
    }

    fn ensure_catalog(&mut self, ctx: &egui::Context) {
        let now = ctx.input(|i| i.time);
        if self.catalog_job.is_some() || now < self.catalog_retry {
            return;
        }
        let Some(paths) = self.paths.clone() else { return };
        // Without a catalog it is built; with one only when a patch or the emergency limit made it
        // stale. Either way the next look is half an hour away, a failed build included.
        self.catalog_retry = now + CATALOG_LOOK_SECONDS;
        if self.catalog.ready() && !shops::catalog_due(&paths, trade::now(), None) {
            return;
        }
        let (done, total) = (Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0)));
        let (counter, all, wake) = (done.clone(), total.clone(), ctx.clone());
        self.catalog_job = Some((
            Job::spawn(ctx, "shops-catalog", move || {
                let fetch = trade::fetcher();
                let now = trade::now();
                let built = shops::build_catalog(&paths, now, None, &fetch, &mut |step, of| {
                    counter.store(step, Ordering::Relaxed);
                    all.store(of, Ordering::Relaxed);
                    wake.request_repaint();
                });
                // The ships belong to the list: without them the area "Ships" is missing.
                let _ = ship_market::update(&paths, now, &fetch);
                built
            }),
            done,
            total,
        ));
    }

    /// Without a catalog and without a fetch of it, the blueprints are read once in the background from the
    /// kept game values (nothing is asked of the game files or the network).
    fn ensure_fallback(&mut self, ctx: &egui::Context) {
        if self.catalog.ready() || self.catalog_job.is_some() || self.fallback.is_some() || self.fallback_job.is_some()
        {
            return;
        }
        let Some(paths) = self.paths.clone() else { return };
        self.fallback_job = Some(Job::spawn(ctx, "shops-blueprints", move || {
            let values = values_store::kept(&paths.app_file(values_store::CACHE_FILE)).unwrap_or_default();
            blueprint_entries(&CraftBook::build(&values))
        }));
    }

    /// Whether a part or a ship is open on top of the list.
    pub fn detail_open(&self) -> bool {
        self.chosen.is_some()
    }

    /// Back to the list of results, with the search and the menus as they were.
    pub fn close_detail(&mut self) {
        self.chosen = None;
        self.added = None;
        self.planned = None;
    }

    /// Puts `amount` of the open part into the shopping list and says so.
    fn add_to_basket(&mut self, chosen: &Chosen) {
        let Some(paths) = self.paths.clone() else { return };
        let mut held = basket::load(&paths);
        held.add(&chosen.code, &chosen.name, self.amount);
        let count = held.items.iter().find(|i| i.reference == chosen.code).map_or(0, |i| i.count);
        self.added = Some(match basket::save(&paths, &held) {
            Ok(()) => (tf("s_ld_im_korb", &[("0", Arg::from(count)), ("1", Arg::from(chosen.name.as_str()))]), false),
            Err(error) => {
                versekit_core::errors::record("shops.basket", &error);
                (t("s_ld_korb_fehler"), true)
            }
        });
    }

    fn choose(&mut self, ctx: &egui::Context, entry: &Entry) {
        // The search stays: the way back leads to the same results (4.12 emptied it here).
        self.chosen = Some(Chosen { code: entry.code.clone(), name: entry.name.clone() });
        self.amount = 1;
        self.added = None;
        self.planned = None;
        if entry.is_ship() || self.shops.known(&entry.code) {
            return;
        }
        if self.part_job.is_some() {
            // One request at a time; this one follows when the running one is done.
            self.queued_part = Some((entry.code.clone(), entry.name.clone()));
            return;
        }
        self.start_part(ctx, &entry.code, &entry.name);
    }

    fn start_part(&mut self, ctx: &egui::Context, code: &str, name: &str) {
        if let Some(paths) = self.paths.clone() {
            let (code, name) = (code.to_string(), name.to_string());
            self.part_job = Some((
                code.clone(),
                Job::spawn(ctx, "shops-part", move || {
                    let mut book = Shops::load(&paths);
                    let mut catalog = Catalog::load(&paths);
                    shops::fetch_part(
                        &paths,
                        &mut book,
                        &mut catalog,
                        (&code, &name),
                        false,
                        trade::now(),
                        &trade::fetcher(),
                    )
                }),
            ));
        }
    }

    fn poll_jobs(&mut self, ui: &Ui) {
        let mut reload = false;
        if let Some(job) = &self.fallback_job {
            match job.poll() {
                Polled::Running => {}
                Polled::Done(entries) => {
                    self.fallback = Some(entries);
                    self.fallback_job = None;
                    reload = true;
                }
                Polled::Lost => {
                    self.fallback = Some(Vec::new());
                    self.fallback_job = None;
                }
            }
        }
        if let Some((job, _, _)) = &self.catalog_job
            && !matches!(job.poll(), Polled::Running)
        {
            self.catalog_job = None;
            reload = true;
        }
        if let Some((code, job)) = &self.part_job {
            match job.poll() {
                Polled::Running => {}
                Polled::Done(true) => {
                    self.part_job = None;
                    reload = true;
                }
                Polled::Done(false) | Polled::Lost => {
                    self.failed.push(code.clone());
                    self.part_job = None;
                }
            }
        }
        if reload {
            self.reload();
            self.normalize();
            ui.ctx().request_repaint();
        }
        if self.part_job.is_none()
            && let Some((code, name)) = self.queued_part.take()
            && !self.shops.known(&code)
        {
            let ctx = ui.ctx().clone();
            self.start_part(&ctx, &code, &name);
        }
    }
}

// ------------------------------------------------------------ names

/// The readable name of a value of a menu.
fn label_of(field: Field, value: &str) -> String {
    match field {
        Field::Area if value == AREA_SHIPS => t("s_ld_ber_schiffe"),
        Field::Area => shops::section_label(value),
        Field::Group => match value.strip_prefix(SHIPYARD_PREFIX) {
            Some(yard) => yard.to_string(),
            None => shops::group_label(value),
        },
        Field::Size => tf("s_ld_groesse", &[("0", Arg::from(value))]),
        Field::Class => class_key(value).map_or_else(|| value.to_string(), t),
        Field::Grade => tf("s_ld_guete", &[("0", Arg::from(value))]),
    }
}

fn all_label(field: Field) -> String {
    t(match field {
        Field::Area => "s_ld_alle_bereiche",
        Field::Group => "s_ld_alle_arten",
        Field::Size => "s_ld_alle_groessen",
        Field::Class => "s_ld_alle_klassen",
        Field::Grade => "s_ld_alle_gueten",
    })
}

/// What the field of the place offers: the places, then the names of the shops that are no place of the same name.
fn place_suggestions(places: &[String], outlets: &[String]) -> Vec<String> {
    let mut all = places.to_vec();
    all.extend(outlets.iter().filter(|name| !places.iter().any(|p| lower(p) == lower(name))).cloned());
    all
}

/// A number of parts with the thousands separator of the language.
fn counted(n: usize) -> String {
    versekit_core::formats::count(language::current(), i64::try_from(n).unwrap_or(i64::MAX))
}

// ------------------------------------------------------------ the page

/// The page; `hangar` holds the ships planned, which the detail view of a ship adds to.
pub fn show_page(ui: &mut Ui, scheme: &Scheme, state: &mut ShopsState, hangar: &mut HangarState) -> Option<Action> {
    state.follow(ui);
    state.poll_jobs(ui);
    let ctx = ui.ctx().clone();
    state.ensure_catalog(&ctx);
    state.ensure_fallback(&ctx);
    trade::quiet(ui, scheme, &t("s_hd_lead_ld"));
    ui.add_space(4.0);
    head(ui, scheme, state);
    ui.add_space(GAP);
    let row_height = ui.text_style_height(&egui::TextStyle::Body) + 10.0;
    let room = layout::list_room(ui, row_height) - widgets::LIST_FRAME_EXTRA;
    let area = egui::ScrollArea::vertical()
        .id_salt(LIST_ID)
        .max_height(room)
        .min_scrolled_height(room)
        .auto_shrink([false, true]);
    let mut action = None;
    let shown = widgets::list_frame_marked(ui, scheme, |ui| {
        widgets::scroll_area(ui, scheme, area, |ui| {
            ui.set_max_width(ui.available_width() - layout::SAFETY);
            action = body(ui, scheme, state, hangar);
        })
    })
    .inner;
    state.list_overflow = trade::overflow_of(&shown);
    action
}

fn head(ui: &mut Ui, scheme: &Scheme, state: &mut ShopsState) {
    let hint = t("s_hd_ld_suche");
    let width = ui.available_width() - layout::SAFETY;
    let response = widgets::search_field(ui, &mut state.search, &hint, scheme, width);
    if response.changed() && !state.search.is_empty() {
        state.chosen = None;
    }
    ui.add_space(4.0);
    // The place, or a shop by its name: typed or picked. A place counts once the text is exactly one place; the name
    // of one shop opens its whole offer at its place.
    let places = state.catalog.places();
    let suggestions = place_suggestions(&places, &state.catalog.outlet_names());
    let more = |n: usize| tf("s_mlg_mehr", &[("n", Arg::from(n))]);
    let place_hint = t("s_hd_ld_ort_ph");
    let mut text = std::mem::take(&mut state.place_text);
    let typed_before = text.clone();
    ui.label(RichText::new(t("s_ld_ort")).strong());
    let picked = widgets::SuggestField::new("handel-ort", &place_hint, scheme, &more)
        .width(width)
        .show(ui, &mut text, &suggestions)
        .picked;
    if let Some(index) = picked
        && let Some(name) = suggestions.get(index)
    {
        text = name.clone();
    }
    let changed = text != typed_before;
    state.place_text = text;
    let before = (state.place.clone(), state.outlet.clone());
    state.take_place_text(changed);
    if before != (state.place.clone(), state.outlet.clone()) {
        state.chosen = None;
        state.normalize();
    }
    if !state.place_text_known() {
        trade::colored(ui, scheme.yellow, &t("s_ld_ort_unbekannt"));
    }
    ui.add_space(4.0);
    menus(ui, state);
}

fn menus(ui: &mut Ui, state: &mut ShopsState) {
    let mut words: Vec<Vec<String>> = Vec::new();
    let mut values: Vec<Vec<String>> = Vec::new();
    let mut shown: Vec<usize> = Vec::new();
    for (index, field) in FIELDS.iter().enumerate() {
        let menu = state.menu(index);
        if menu.is_empty() {
            continue;
        }
        let mut list = vec![all_label(*field)];
        let mut raw = vec![String::new()];
        for (value, count) in menu {
            list.push(format!("{} ({})", label_of(*field, &value), counted(count)));
            raw.push(value);
        }
        words.push(list);
        values.push(raw);
        shown.push(index);
    }
    if shown.is_empty() {
        return;
    }
    let refs: Vec<Vec<&str>> = words.iter().map(|w| w.iter().map(String::as_str).collect()).collect();
    let slices: Vec<&[&str]> = refs.iter().map(Vec::as_slice).collect();
    let mut chosen: Vec<usize> = shown
        .iter()
        .enumerate()
        .map(|(n, index)| values[n].iter().position(|v| *v == state.wanted[*index]).unwrap_or(0))
        .collect();
    if widgets::filter_bar(ui, "laeden", &slices, &mut chosen) {
        for (n, index) in shown.iter().enumerate() {
            state.wanted[*index] = values[n][chosen[n]].clone();
        }
        state.chosen = None;
        state.normalize();
    }
}

fn body(ui: &mut Ui, scheme: &Scheme, state: &mut ShopsState, hangar: &mut HangarState) -> Option<Action> {
    let lang = language::current();
    // A catalog that is there stays on screen while a new one is built.
    if let Some((_, done, total)) = state.catalog_job.as_ref().filter(|_| !state.catalog.ready()) {
        let text = if total.load(Ordering::Relaxed) > 0 {
            tf(
                "s_ld_katalog_stand",
                &[("0", Arg::from(done.load(Ordering::Relaxed))), ("1", Arg::from(total.load(Ordering::Relaxed)))],
            )
        } else {
            t("s_hd_ld_katalog")
        };
        widgets::loading(ui, scheme, &text);
        return None;
    }
    if !state.catalog.ready() {
        let key = if state.paths.is_some() { "s_hd_ld_ohne_katalog" } else { "s_hd_ld_ohne_daten" };
        if state.entries.is_empty() {
            widgets::empty_state(ui, scheme, "store", &t(key));
            return None;
        }
        // The blueprints stand in for the catalog; the line says why.
        trade::colored(ui, scheme.gold, &t(key));
        ui.add_space(4.0);
    }
    if let Some(chosen) = state.chosen.clone() {
        // From a part of a shop the way back leads to that shop.
        let back = match &state.outlet {
            Some(outlet) => tf("s_ld_zurueck_zu", &[("0", Arg::from(outlet.name.as_str()))]),
            None => t("s_ld_zurueck_liste"),
        };
        if ui.add(widgets::Button::new(&back, ButtonKind::Normal, scheme)).clicked() {
            state.close_detail();
            return None;
        }
        ui.add_space(4.0);
        let mut action = None;
        if chosen.code.starts_with(SHIP_PREFIX) {
            action = plan_row(ui, scheme, state, hangar, &chosen);
            ui.add_space(4.0);
        } else if state.paths.is_some() {
            basket_row(ui, scheme, state, &chosen);
            ui.add_space(4.0);
        }
        chosen_view(ui, scheme, state, &chosen, lang);
        return action;
    }
    if state.outlet.is_some() {
        match state.outlet_index() {
            Some(at) => {
                outlet_view(ui, scheme, state, at, lang);
                return None;
            }
            // The catalog read again no longer knows the shop: back to the list.
            None => state.outlet = None,
        }
    }
    list_view(ui, scheme, state);
    None
}

/// The shops of the chosen place, each with how many parts it sells; a click opens its whole offer. A catalog of
/// the format before has no shops yet: a line says when they come.
fn outlets_of_place(ui: &mut Ui, scheme: &Scheme, state: &mut ShopsState) {
    let Some(place) = state.place.clone() else { return };
    if !state.catalog.knows_outlets() {
        trade::quiet(ui, scheme, &t("s_ld_shops_spaeter"));
        ui.add_space(GAP);
        return;
    }
    // Counted as the offer of a shop shows it: with the search and the menus.
    let outlets: Vec<(usize, usize)> = state
        .catalog
        .outlets_at(&place)
        .into_iter()
        .map(|(at, _)| (at, state.outlet_offers(at).len()))
        .filter(|(_, count)| *count > 0)
        .collect();
    if outlets.is_empty() {
        return;
    }
    ui.label(
        RichText::new(tf("s_ld_shops_hier", &[("0", Arg::from(place.as_str())), ("1", Arg::from(counted(outlets.len())))]))
            .strong()
            .color(color(scheme.accent)),
    );
    let mut opened = None;
    for (n, (at, count)) in outlets.iter().enumerate() {
        let name = state.catalog.outlet_label(*at);
        let text = format!("{name} ({})", tf("s_ld_shop_teile", &[("0", Arg::from(counted(*count)))]));
        if trade::link_row_striped(ui, scheme, n % 2 == 1, &text) {
            opened = Some(*at);
        }
    }
    ui.add_space(GAP);
    if let Some(at) = opened {
        state.open_outlet(at);
    }
}

/// The whole offer of one shop: its name, place and system, then every part it sells with its price there, by
/// group; a click on a part opens it with all its shops and the shopping list, as in the list.
fn outlet_view(ui: &mut Ui, scheme: &Scheme, state: &mut ShopsState, at: usize, lang: Lang) {
    let outlet = state.catalog.outlets[at].clone();
    let back = tf("s_ld_zurueck_zu", &[("0", Arg::from(outlet.place.as_str()))]);
    if ui.add(widgets::Button::new(&back, ButtonKind::Normal, scheme)).clicked() {
        state.outlet = None;
        return;
    }
    ui.add_space(4.0);
    let offers = state.outlet_offers(at);
    trade::two_sided(
        ui,
        |left| {
            left.label(RichText::new(&outlet.name).strong());
        },
        |side| {
            let place: Vec<&str> =
                [outlet.place.as_str(), outlet.system.as_str()].into_iter().filter(|s| !s.is_empty()).collect();
            trade::quiet(side, scheme, &place.join(" · "));
        },
    );
    trade::quiet(ui, scheme, &tf("s_ld_shop_teile", &[("0", Arg::from(counted(offers.len())))]));
    ui.add_space(4.0);
    if offers.is_empty() {
        trade::quiet(ui, scheme, &t("s_hd_ld_nichts"));
        return;
    }
    let mut groups: Vec<(String, Vec<(usize, f64)>)> = Vec::new();
    for offer in offers {
        let group = state.entries[offer.0].group.clone();
        match groups.iter_mut().find(|(name, _)| *name == group) {
            Some(slot) => slot.1.push(offer),
            None => groups.push((group, vec![offer])),
        }
    }
    groups.sort_by_cached_key(|(group, _)| lower(&label_of(Field::Group, group)));
    let mut picked = None;
    let mut shown = 0;
    for (group, members) in &groups {
        if shown >= EMERGENCY_CAP {
            break;
        }
        ui.label(
            RichText::new(format!("{} ({})", label_of(Field::Group, group), counted(members.len())))
                .strong()
                .color(color(scheme.accent)),
        );
        for (n, (index, price)) in members.iter().take(EMERGENCY_CAP - shown).enumerate() {
            if offer_row(ui, scheme, &state.entries[*index], *price, n % 2 == 1, lang) {
                picked = Some(*index);
            }
            shown += 1;
        }
        ui.add_space(GAP);
    }
    if let Some(index) = picked {
        let entry = state.entries[index].clone();
        let ctx = ui.ctx().clone();
        state.choose(&ctx, &entry);
    }
}

/// A row of the offer of a shop: the name, and the details and the price there at the right.
fn offer_row(ui: &mut Ui, scheme: &Scheme, entry: &Entry, price: f64, stripe: bool, lang: Lang) -> bool {
    let height = ui.text_style_height(&egui::TextStyle::Body) + 10.0;
    let (rect, clicked) = trade::clickable_row(ui, scheme, stripe, height);
    let mut details = entry_details(entry);
    details.push(trade::auec(lang, price));
    let right = RichText::new(details.join(" · ")).small().color(color(scheme.sub));
    let mut inner = trade::row_content(ui, rect, Some(right));
    inner.add(egui::Label::new(&entry.name).truncate());
    clicked
}

/// Where the basket row stands for a part, a ship has the button that plans it in the hangar (the section
/// "Planned"), or, when it is planned already, the line that says so; below it what the last press did.
/// Both lead to the section.
fn plan_row(
    ui: &mut Ui,
    scheme: &Scheme,
    state: &mut ShopsState,
    hangar: &mut HangarState,
    chosen: &Chosen,
) -> Option<Action> {
    hangar.follow_planned(ui);
    let mut action = None;
    let planned = hangar.is_planned(&chosen.name);
    let jump = |row: &mut Ui| {
        row.add(widgets::Button::new(&t("s_ld_zu_geplant"), ButtonKind::Normal, scheme).icon("plane")).clicked()
    };
    if !planned {
        ui.horizontal_wrapped(|row| {
            let word = t("s_ld_vormerken");
            if row.add(widgets::Button::new(&word, ButtonKind::Strong, scheme).icon("plus")).clicked() {
                state.planned = Some(hangar.plan_ship_said(&chosen.name));
            }
        });
    } else if state.planned.is_none() {
        ui.horizontal_wrapped(|row| {
            row.add(egui::Label::new(RichText::new(t("s_ld_schon_geplant")).color(color(scheme.accent))).wrap());
            if jump(row) {
                action = Some(Action::ToPlanned);
            }
        });
    }
    if let Some((said, failed)) = &state.planned {
        ui.horizontal_wrapped(|row| {
            let tone = if *failed { scheme.red_text() } else { scheme.accent };
            row.add(egui::Label::new(RichText::new(said).color(color(tone))).wrap());
            if !*failed && jump(row) {
                action = Some(Action::ToPlanned);
            }
        });
    }
    action
}

/// How many of the open part, and the button that puts them into the shopping list; below it what the
/// last press did.
fn basket_row(ui: &mut Ui, scheme: &Scheme, state: &mut ShopsState, chosen: &Chosen) {
    ui.horizontal_wrapped(|row| {
        let label = row.label(t("s_ld_menge"));
        widgets::number_field_labelled(row, scheme, "korb-menge", &mut state.amount, 1..=999, "", Some(label.id));
        if row.add(widgets::Button::new(&t("s_ld_in_korb"), ButtonKind::Strong, scheme)).clicked() {
            state.add_to_basket(chosen);
        }
    });
    if let Some((said, failed)) = &state.added {
        let tone = if *failed { scheme.red_text() } else { scheme.accent };
        ui.add(egui::Label::new(RichText::new(said).color(color(tone))).wrap());
    }
}

/// The list: the areas without input, otherwise the parts that fit.
fn list_view(ui: &mut Ui, scheme: &Scheme, state: &mut ShopsState) {
    let ctx = ui.ctx().clone();
    let filtered = state.place.is_some() || state.wanted.iter().any(|w| !w.is_empty());
    let searching = !state.search.trim().is_empty();
    if !searching && !filtered {
        areas(ui, scheme, state);
        return;
    }
    // A place without a search shows its shops first, then what is sold there.
    if !searching {
        outlets_of_place(ui, scheme, state);
        if state.outlet.is_some() {
            return;
        }
    }
    let shown = state.visible();
    let offered = state.entries.iter().filter(|e| state.at_place(e)).count();
    let ships = state.entries.iter().filter(|e| state.at_place(e) && e.is_ship()).count();
    let parts = offered - ships;
    let line = if ships > 0 {
        tf("s_ld_nur_kaufbar_schiffe", &[("0", Arg::from(parts)), ("1", Arg::from(ships))])
    } else {
        tf("s_ld_nur_kaufbar", &[("0", Arg::from(parts))])
    };
    trade::quiet(ui, scheme, &line);
    if shown.is_empty() {
        trade::quiet(ui, scheme, &t("s_hd_ld_nichts"));
        return;
    }
    // Groups: by count, ties by raw name; one group is a flat list.
    let mut groups: Vec<(String, Vec<usize>)> = Vec::new();
    for index in &shown {
        let group = state.entries[*index].group.clone();
        match groups.iter_mut().find(|(name, _)| *name == group) {
            Some(slot) => slot.1.push(*index),
            None => groups.push((group, vec![*index])),
        }
    }
    let mut picked: Option<usize> = None;
    if groups.len() == 1 {
        let all = &groups[0].1;
        for (n, index) in all.iter().take(EMERGENCY_CAP).enumerate() {
            if part_row(ui, scheme, &state.entries[*index], n % 2 == 1) {
                picked = Some(*index);
            }
        }
        if all.len() > EMERGENCY_CAP {
            trade::quiet(
                ui,
                scheme,
                &tf("s_hd_ld_mehr_da", &[("0", Arg::from(EMERGENCY_CAP)), ("1", Arg::from(all.len()))]),
            );
        }
    } else {
        groups.sort_by(|a, b| b.1.len().cmp(&a.1.len()).then_with(|| lower(&a.0).cmp(&lower(&b.0))));
        let mut open_group: Option<String> = None;
        for (group, members) in &groups {
            ui.label(
                RichText::new(format!("{} ({})", label_of(Field::Group, group), counted(members.len())))
                    .strong()
                    .color(color(scheme.accent)),
            );
            for (n, index) in members.iter().take(PER_GROUP).enumerate() {
                if part_row(ui, scheme, &state.entries[*index], n % 2 == 1) {
                    picked = Some(*index);
                }
            }
            if members.len() > PER_GROUP {
                let text = tf("s_ld_weitere", &[("0", Arg::from(members.len() - PER_GROUP))]);
                if trade::link_row(ui, scheme, &text) {
                    open_group = Some(group.clone());
                }
            }
            ui.add_space(GAP);
        }
        if let Some(group) = open_group {
            // The group becomes a menu choice when the menu offers it.
            let menu_has = state.menu(1).iter().any(|(value, _)| *value == group);
            if menu_has {
                // The search stays, as in 4.12.
                state.wanted[1] = group;
            }
        }
    }
    if let Some(index) = picked {
        let entry = state.entries[index].clone();
        state.choose(&ctx, &entry);
    }
}

/// Without input: the areas with their counts, ships first.
fn areas(ui: &mut Ui, scheme: &Scheme, state: &mut ShopsState) {
    trade::quiet(ui, scheme, &t("s_hd_ld_bereiche"));
    let mut counts = state.menu(0);
    counts.sort_by_key(|(value, _)| value != AREA_SHIPS);
    for (n, (value, count)) in counts.iter().enumerate() {
        let text = format!("{} ({})", label_of(Field::Area, value), counted(*count));
        if trade::link_row_striped(ui, scheme, n % 2 == 1, &text) {
            state.wanted[0] = value.clone();
            state.normalize();
        }
    }
}

/// Class, size, grade and maker of an entry, readable, those it has.
fn entry_details(entry: &Entry) -> Vec<String> {
    let mut details: Vec<String> = Vec::new();
    if !entry.class.is_empty() {
        details.push(label_of(Field::Class, &entry.class));
    }
    if !entry.size.is_empty() {
        details.push(label_of(Field::Size, &entry.size));
    }
    if !entry.grade.is_empty() {
        details.push(label_of(Field::Grade, &entry.grade));
    }
    if !entry.maker.is_empty() {
        details.push(entry.maker.clone());
    }
    details
}

/// A row of the list: the name, and class, size, grade and maker at the right.
fn part_row(ui: &mut Ui, scheme: &Scheme, entry: &Entry, stripe: bool) -> bool {
    let height = ui.text_style_height(&egui::TextStyle::Body) + 10.0;
    let (rect, clicked) = trade::clickable_row(ui, scheme, stripe, height);
    let details = entry_details(entry);
    let right = (!details.is_empty()).then(|| RichText::new(details.join(" · ")).small().color(color(scheme.sub)));
    let mut inner = trade::row_content(ui, rect, right);
    inner.add(egui::Label::new(&entry.name).truncate());
    clicked
}

// ------------------------------------------------------------ one part or ship

fn chosen_view(ui: &mut Ui, scheme: &Scheme, state: &mut ShopsState, chosen: &Chosen, lang: Lang) {
    if chosen.code.starts_with(SHIP_PREFIX) {
        ship_view(ui, scheme, state, chosen, lang);
        return;
    }
    let now = trade::now();
    let Some(rows) = state.shops.shops_for(&chosen.code).map(<[shops::ShopRow]>::to_vec) else {
        if state.failed.contains(&chosen.code) && state.part_job.is_none() {
            trade::quiet(ui, scheme, &t("s_vk_fehler"));
        } else {
            widgets::loading(ui, scheme, &t("s_ld_sucht"));
        }
        return;
    };
    trade::two_sided(
        ui,
        |left| {
            left.label(RichText::new(&chosen.name).strong());
        },
        |side| {
            if let Some(age) = state.shops.age(&chosen.code, now) {
                trade::quiet(side, scheme, &tf("s_vk_stand", &[("alter", Arg::from(trade::age_text(Some(age))))]));
            }
        },
    );
    ui.add_space(4.0);
    if rows.is_empty() {
        trade::quiet(ui, scheme, &t("s_hd_ld_unbekannt"));
        return;
    }
    let mut shown = rows.clone();
    if let Some(place) = state.place.clone() {
        let here: Vec<shops::ShopRow> = rows.iter().filter(|r| r.place == place).cloned().collect();
        if here.is_empty() {
            trade::quiet(ui, scheme, &tf("s_hd_ld_ort_nicht_hier", &[("0", Arg::from(place.as_str()))]));
        } else {
            shown = here;
        }
    }
    for (n, row) in shown.iter().enumerate() {
        widgets::list_frame(ui, scheme, |ui| {
            ui.set_min_width(ui.available_width() - layout::SAFETY);
            ui.horizontal_wrapped(|line| {
                let tone = if n == 0 { scheme.accent } else { scheme.fg };
                line.label(RichText::new(trade::auec(lang, row.price)).strong().color(color(tone)));
                line.label(if row.shop.is_empty() { "?" } else { &row.shop });
                let place: Vec<&str> =
                    [row.place.as_str(), row.system.as_str()].into_iter().filter(|s| !s.is_empty()).collect();
                if !place.is_empty() {
                    line.label(RichText::new(place.join(" · ")).color(color(scheme.sub)));
                }
                if row.condition != 0 && row.condition < 100 {
                    line.label(
                        RichText::new(tf("s_ld_zustand", &[("0", Arg::from(row.condition))]))
                            .color(color(scheme.yellow)),
                    );
                }
            });
        });
        ui.add_space(2.0);
    }
}

fn ship_view(ui: &mut Ui, scheme: &Scheme, state: &mut ShopsState, chosen: &Chosen, lang: Lang) {
    let scu = state.market.scu(&chosen.name);
    trade::two_sided(
        ui,
        |left| {
            left.label(RichText::new(&chosen.name).strong());
        },
        |side| {
            if scu > 0 {
                trade::quiet(side, scheme, &tf("s_ld_scu", &[("0", Arg::from(scu))]));
            }
        },
    );
    ui.add_space(4.0);
    let mut any = false;
    for (key, offers) in
        [("s_ld_kaufen", state.market.buy_at(&chosen.name)), ("s_ld_mieten", state.market.rent_at(&chosen.name))]
    {
        if offers.is_empty() {
            continue;
        }
        any = true;
        ui.label(RichText::new(t(key)).strong());
        for (n, offer) in offers.iter().enumerate() {
            widgets::list_frame(ui, scheme, |ui| {
                ui.set_min_width(ui.available_width() - layout::SAFETY);
                ui.horizontal_wrapped(|line| {
                    let tone = if n == 0 { scheme.accent } else { scheme.fg };
                    line.label(RichText::new(trade::auec(lang, offer.price)).strong().color(color(tone)));
                    line.label(if offer.spot.is_empty() { "?" } else { &offer.spot });
                    let place: Vec<&str> =
                        [offer.place.as_str(), offer.system.as_str()].into_iter().filter(|s| !s.is_empty()).collect();
                    if !place.is_empty() {
                        line.label(RichText::new(place.join(" · ")).color(color(scheme.sub)));
                    }
                });
            });
            ui.add_space(2.0);
        }
        ui.add_space(GAP);
    }
    if !any {
        trade::quiet(ui, scheme, &t("s_ld_schiff_nichts"));
    }
}

/// Hooks for the tests of the page.
#[cfg(test)]
impl ShopsState {
    pub(crate) fn pick_by_name(&mut self, name: &str) {
        if let Some(entry) = self.entries.iter().find(|e| e.name == name).cloned() {
            self.choose(&egui::Context::default(), &entry);
        }
    }

    pub(crate) fn want(&mut self, field: usize, value: &str) {
        self.wanted[field] = value.to_string();
        self.normalize();
    }

    pub(crate) fn want_place(&mut self, place: &str) {
        self.place = Some(place.to_string());
        self.place_text = place.to_string();
        self.normalize();
    }

    /// The field of the place as somebody types `text` into it.
    pub(crate) fn type_place(&mut self, text: &str) {
        self.place_text = text.to_string();
        self.take_place_text(true);
        self.normalize();
    }

    /// The name of the open shop, if one is open.
    pub(crate) fn open_outlet_name(&self) -> Option<String> {
        self.outlet.as_ref().map(|o| o.name.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &str, area: &str, group: &str, size: &str, class: &str, places: &[&str]) -> Entry {
        Entry {
            name: name.into(),
            code: format!("u-{name}"),
            area: area.into(),
            group: group.into(),
            maker: String::new(),
            size: size.into(),
            class: class.into(),
            grade: String::new(),
            places: places.iter().map(|p| p.to_string()).collect(),
            hay: String::new(),
        }
    }

    fn state() -> ShopsState {
        let mut state = ShopsState {
            entries: vec![
                entry("Arctic", "Systems", "Coolers", "2", "Military", &["Lorville"]),
                entry("Frost", "Systems", "Coolers", "10", "Civilian", &["Area18"]),
                entry("Wave", "Systems", "Coolers", "1", "Civilian", &["Lorville"]),
                entry("Bolt", "Systems", "Power Plants", "2", "Military", &["Lorville"]),
                entry("Helmet", "Armor", "Helmets", "", "", &["Lorville"]),
                entry("Hornet", AREA_SHIPS, "werft:Anvil", "", "", &[]),
            ],
            ..ShopsState::default()
        };
        state.refresh_hay();
        state
    }

    fn values(state: &ShopsState, index: usize) -> Vec<String> {
        state.menu(index).into_iter().map(|(value, _)| value).collect()
    }

    #[test]
    fn the_parts_go_by_size_then_name() {
        // As the catalog holds them: by name. S10 after S2, a part without a size last.
        let mut entries = vec![
            entry("Aaron Helmet", "Armor", "Helmets", "", "", &[]),
            entry("Omnisky III Cannon", "Weapons", "Guns", "1", "", &[]),
            entry("Omnisky IX Cannon", "Weapons", "Guns", "3", "", &[]),
            entry("Omnisky VI Cannon", "Weapons", "Guns", "2.0", "", &[]),
            entry("Omnisky XII Cannon", "Weapons", "Guns", "10", "", &[]),
        ];
        in_size_order(&mut entries);
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(
            names,
            ["Omnisky III Cannon", "Omnisky VI Cannon", "Omnisky IX Cannon", "Omnisky XII Cannon", "Aaron Helmet"]
        );
    }

    #[test]
    fn each_menu_offers_only_what_fits_the_ones_before() {
        let mut state = state();
        assert_eq!(values(&state, 0).len(), 3, "areas");
        state.want(0, "Armor");
        // Armor has no size and no class: those menus are empty and drop out.
        assert_eq!(values(&state, 1), vec!["Helmets"]);
        assert!(values(&state, 2).is_empty());
        state.want(0, "Systems");
        assert_eq!(values(&state, 1), vec!["Coolers", "Power Plants"], "by count");
        // Sizes by number, not by text: 10 after 2.
        state.want(1, "Coolers");
        assert_eq!(values(&state, 2), vec!["1", "2", "10"]);
        // Counter-check: sorted as text it would put 10 first.
        let mut as_text = values(&state, 2);
        as_text.sort();
        assert_ne!(as_text, values(&state, 2));
    }

    #[test]
    fn a_choice_that_no_longer_fits_drops_out() {
        let mut state = state();
        state.want(0, "Systems");
        state.want(1, "Power Plants");
        state.want(2, "2");
        assert_eq!(state.wanted[2], "2");
        // Another area: the group is not there any more, and the size chosen after it goes too.
        state.wanted[0] = "Armor".into();
        state.normalize();
        assert_eq!(state.wanted[1], "");
        assert_eq!(state.wanted[2], "");
    }

    /// The state of [`state`] with a catalog that knows the shops: Cool Corner and CenterMass in Lorville,
    /// Dumpers in Area18, and a shop named like a place.
    fn state_with_shops() -> ShopsState {
        let mut state = state();
        let data = versekit_core::jsonio::loads(
            r#"{"teile": [
                {"n": "Arctic", "s": "u-Arctic", "o": ["Lorville"], "l": [[0, 100.0], [1, 120.0]]},
                {"n": "Frost", "s": "u-Frost", "o": ["Area18"], "l": [[2, 90.0]]},
                {"n": "Wave", "s": "u-Wave", "o": ["Lorville"], "l": [[0, 50.0]]},
                {"n": "Helmet", "s": "u-Helmet", "o": ["Lorville"], "l": [[1, 300.0]]}],
             "laeden": [["Cool Corner", "Lorville", "Stanton"], ["CenterMass", "Lorville", "Stanton"],
                        ["Dumpers", "Area18", "Stanton"], ["Lorville", "Area18", "Stanton"]]}"#,
        )
        .unwrap();
        state.catalog = Catalog::from_value(&data);
        state
    }

    #[test]
    fn the_field_offers_places_and_shops_and_the_name_of_a_shop_opens_it() {
        let mut state = state_with_shops();
        let names = place_suggestions(&state.catalog.places(), &state.catalog.outlet_names());
        assert_eq!(names, ["Area18", "Lorville", "CenterMass", "Cool Corner", "Dumpers"], "a shop named like a place once");
        state.type_place("centermass");
        assert_eq!(state.open_outlet_name().as_deref(), Some("CenterMass"));
        assert_eq!(state.place.as_deref(), Some("Lorville"), "at its place");
        let offer: Vec<(String, f64)> =
            state.outlet_offers(state.outlet_index().unwrap()).into_iter().map(|(i, p)| (state.entries[i].name.clone(), p)).collect();
        assert_eq!(offer, [("Arctic".to_string(), 120.0), ("Helmet".to_string(), 300.0)], "what it sells, its price there");
        // The search narrows the offer as it narrows the list.
        state.search = "helm".into();
        assert_eq!(state.outlet_offers(state.outlet_index().unwrap()).len(), 1);
        state.search.clear();
        // A place typed closes the shop; a word that names nothing leaves the place; empty clears both.
        state.type_place("Area18");
        assert_eq!((state.outlet.is_none(), state.place.as_deref()), (true, Some("Area18")));
        state.type_place("Nirgendwo");
        assert_eq!(state.place.as_deref(), Some("Area18"));
        assert!(!state.place_text_known(), "the field says the word is unknown");
        state.type_place("centermass");
        state.type_place("centermass x");
        assert!(state.outlet.is_none() && !state.place_text_known(), "a word after the name closes the shop");
        state.type_place("");
        assert!(state.place.is_none() && state.outlet.is_none());
        // A shop opened from the list stays while the text of its place stays.
        state.type_place("Lorville");
        state.open_outlet(0);
        state.take_place_text(false);
        assert_eq!(state.open_outlet_name().as_deref(), Some("Cool Corner"));
        assert!(state.is_narrowed());
        state.reset();
        assert!(state.outlet.is_none() && !state.is_narrowed());
    }

    #[test]
    fn the_place_narrows_menus_and_list() {
        let mut state = state();
        state.place = Some("Area18".into());
        assert_eq!(state.visible().len(), 1, "one part at Area18");
        assert_eq!(values(&state, 1), vec!["Coolers"]);
        // Ships have no places and fall out once a place is set.
        assert!(!values(&state, 0).iter().any(|v| v == AREA_SHIPS));
    }

    #[test]
    fn the_search_finds_name_maker_and_group_in_both_languages() {
        let mut state = state();
        state.search = "arc".into();
        assert_eq!(state.visible().len(), 1);
        state.search = "coolers".into();
        assert_eq!(state.visible().len(), 3);
        state.search = "a".into();
        assert!(state.visible().is_empty(), "one letter finds nothing");
    }

    #[test]
    fn the_way_back_from_a_part_leads_to_the_same_results() {
        let mut state = state();
        state.want(0, "Systems");
        state.search = "cool".into();
        let results = state.visible();
        assert_eq!(results.len(), 3);
        state.pick_by_name("Arctic");
        assert!(state.detail_open());
        assert_eq!((state.search.as_str(), state.wanted[0].as_str()), ("cool", "Systems"), "search and menu stay");
        state.close_detail();
        assert!(!state.detail_open());
        assert_eq!(state.visible(), results, "the same results");
    }

    #[test]
    fn a_part_goes_into_the_shopping_list_with_how_many() {
        let dir = std::env::temp_dir().join(format!("versekit-shops-korb-{}", versekit_core::testdir::run_tag()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let home = dir.to_string_lossy().into_owned();
        let vars = [(versekit_core::legacy::ENV_HOME, home.as_str()), (versekit_core::legacy::ENV_NO_NET, "1")];
        let paths = Paths::new(versekit_core::paths::Host::new(versekit_core::pypath::Flavor::native(), &vars, &home));
        let mut state = ShopsState { paths: Some(Arc::new(paths.working_copy())), ..state() };
        state.pick_by_name("Helmet");
        let chosen = state.chosen.clone().expect("open");
        state.amount = 3;
        state.add_to_basket(&chosen);
        state.amount = 2;
        state.add_to_basket(&chosen);
        let kept = basket::load(&paths);
        assert_eq!(kept.items.iter().map(|i| (i.reference.as_str(), i.count)).collect::<Vec<_>>(), [("u-Helmet", 5)]);
        let said = state.added.clone().expect("said");
        assert_eq!(said, (tf("s_ld_im_korb", &[("0", Arg::from(5)), ("1", Arg::from("Helmet"))]), false));
        // Another part: the amount starts at one again.
        state.pick_by_name("Bolt");
        assert_eq!((state.amount, state.added.is_none()), (1, true));
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn craft(name: &str, kind: &str, subtype: &str, entity: &str) -> versekit_gamedata::craft_book::CraftItem {
        versekit_gamedata::craft_book::CraftItem {
            basis: name.into(),
            name: name.into(),
            manufacturer: "Maker".into(),
            kind: kind.into(),
            subtype: subtype.into(),
            catalog_kind: String::new(),
            tiers: 1,
            tag: String::new(),
            tags: Vec::new(),
            entity: entity.into(),
            raw_product: None,
            recipe: 0,
        }
    }

    #[test]
    fn without_a_catalog_the_blueprints_stand_in_by_their_entity_code_and_kind() {
        let _lock = crate::language_lock();
        let mut book = CraftBook::default();
        book.items = vec![
            craft("Omnisky", "weapons", "laser", "e-laser"),
            craft("Arrowhead Rifle", "weapons", "sniper", "e-rifle"),
            craft("Odd Gun", "weapons", "", "e-odd"),
            craft("Glacier", "cooler", "", "e-cool"),
            craft("Nameless", "cooler", "", ""),
        ];
        let entries = blueprint_entries(&book);
        let shown: Vec<(&str, &str, &str)> =
            entries.iter().map(|e| (e.name.as_str(), e.code.as_str(), e.group.as_str())).collect();
        // A blueprint without an entity code is left out; the weapons split into ship and hand weapons.
        assert_eq!(
            shown,
            [
                ("Arrowhead Rifle", "e-rifle", shops::GROUP_FPS_WEAPONS),
                ("Glacier", "e-cool", "cooler"),
                ("Odd Gun", "e-odd", "weapons"),
                ("Omnisky", "e-laser", shops::GROUP_SHIP_WEAPONS),
            ]
        );
        assert!(entries.iter().all(|e| e.area.is_empty() && e.maker.is_empty()));
        assert_eq!(label_of(Field::Group, shops::GROUP_SHIP_WEAPONS), t("s_ld_art_schiffswaffen"));
        assert_eq!(label_of(Field::Group, shops::GROUP_FPS_WEAPONS), t("s_ld_art_fpswaffen"));
        assert_eq!(label_of(Field::Group, "cooler"), t("he_art_cooler"));
        assert_eq!(label_of(Field::Group, "Coolers"), t("s_uk_coolers"));
        assert_eq!(label_of(Field::Group, "Unknown Group"), "Unknown Group");

        // No catalog and no fetch: the blueprints are the list, with the group menu.
        let mut state = ShopsState { fallback: Some(entries), ..ShopsState::default() };
        state.rebuild();
        assert_eq!(state.entries.len(), 4);
        state.want(1, shops::GROUP_FPS_WEAPONS);
        assert_eq!(
            state.visible().iter().map(|i| state.entries[*i].name.as_str()).collect::<Vec<_>>(),
            ["Arrowhead Rifle"]
        );
        state.want(1, "");

        // While the catalog is fetched the list stays empty.
        let (hold, wait) = std::sync::mpsc::channel::<()>();
        let job = Job::spawn(&egui::Context::default(), "shops-test", move || wait.recv().is_ok());
        state.catalog_job = Some((job, Arc::default(), Arc::default()));
        state.rebuild();
        assert!(state.entries.is_empty(), "no stand-in during the fetch");
        drop(hold);
        state.catalog_job = None;

        // A catalog that is there wins over the blueprints.
        let data = versekit_core::jsonio::loads(
            r#"{"teile": [{"n": "Bracer", "s": "u-1", "k": 0}], "kategorien": [["Systems", "Coolers"]]}"#,
        )
        .unwrap();
        state.catalog = Catalog::from_value(&data);
        state.rebuild();
        assert_eq!(state.entries.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(), ["Bracer"]);
    }
}
