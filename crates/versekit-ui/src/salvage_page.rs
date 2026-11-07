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

//! The page "Dismantle & Salvage": one search field for a ship and for a part. A ship or wreck shows what it
//! carries from the factory and what the parts would cost in a shop (`pages._salvage` of 4.12); a part shows
//! what the fabricator gives back when it is taken apart (`pages._dismantle`, see [`crate::dismantle_view`]).
//! A click on a part of a wreck opens its calculation on the same page, with a way back to the wreck.
//!
//! The loadout of a wreck comes from erkul, the prices from the shops; both are fetched in the background,
//! and a wreck looked up stands there next time without the net.
//!
//! Unlike 4.12 the page finds a wreck it looked up before again, names the time of the data, says one part
//! and one ship in the singular, and a click while a lookup runs starts no second one. The search field and
//! its buttons stay on top; only the result below scrolls.

use egui::{RichText, Ui};
use versekit_core::erkul_fetch::FnFeed;
use versekit_core::language::{self, Arg, t, tf};
use versekit_core::net::{Net, Web};
use versekit_core::paths::Paths;
use versekit_core::salvage::{self, Missing, Part, Wreck};
use versekit_core::ships::{ErkulShips, UexShips};
use versekit_core::shops::{self, Catalog, Shops};
use versekit_core::stock_data::lower;
use versekit_core::theme::Scheme;
use versekit_gamedata::values::GameValues;

use crate::color;
use crate::crafting_page::CraftingState;
use crate::dismantle_view;
use crate::trade::{self, Job, Polled};
use crate::widgets::{self, ButtonKind};

/// The longest a file of erkul may be, and how long a request may take.
const ERKUL_LIMIT: u64 = 64 * 1024 * 1024;
const ERKUL_SECONDS: u64 = 30;
/// The page ids the usage counters know the two views by.
const VIEW_WRECK: &str = "bergung";
const VIEW_PART: &str = "zerlegen";

/// What the page shows below the buttons.
#[derive(Clone, Debug, PartialEq)]
enum Shown {
    Nothing,
    /// The parts of a wreck under the name of the ship, and when they were looked up.
    Wreck {
        name: String,
        parts: Vec<Part>,
        when: f64,
    },
    /// erkul could not be reached for this ship: trying again later may help.
    Unreachable {
        name: String,
    },
    /// What taking a part apart gives; `from` is the wreck the part was opened from, the way back.
    Part {
        name: String,
        from: Option<Box<Shown>>,
    },
}

impl Shown {
    /// The name the search field has to hold for this result to stand.
    fn name(&self) -> Option<&str> {
        match self {
            Shown::Nothing => None,
            Shown::Wreck { name, .. } | Shown::Unreachable { name } | Shown::Part { name, .. } => Some(name),
        }
    }
}

/// What the text of the field names.
#[derive(Clone, Debug, PartialEq)]
enum Target {
    /// A ship, as the list of ships writes it.
    Ship(String),
    /// A part, as the list of parts writes it.
    Part(String),
    /// Nothing, or more than one thing.
    Unclear,
}

/// The page "Dismantle & Salvage".
pub struct SalvageState {
    paths: Option<Paths>,
    ships: UexShips,
    shops: Shops,
    /// The text of the field.
    pub typed: String,
    /// The text of the field names neither a ship nor a part of the lists.
    no_match: bool,
    shown: Shown,
    /// A line about the last action and whether it is a fault.
    said: Option<(String, bool)>,
    /// The lookup that runs: the name of the ship and the wreck it brings.
    job: Option<(String, Job<Result<Wreck, Missing>>)>,
    /// The question before the wrecks are forgotten: how many there are.
    asking: Option<usize>,
    /// The names of ships, then of parts, as the field offers them.
    entries: Vec<String>,
    /// How many of the entries are ships.
    ship_count: usize,
    /// The game values the part names were taken from (address and number of items); `None` before any.
    parts_from: Option<(usize, usize)>,
    /// A route that asked for the calculation of a part: the usage counters count it so until a result stands.
    wants_part: bool,
    list_overflow: f32,
    /// How far the result is scrolled down.
    list_scrolled: f32,
}

/// The ships of the sample and what the made-up loadout holds.
const SAMPLE_SHIPS: [&str; 3] = ["Cutlass Black", "Prospector", "Avenger Titan"];

/// The parts of the made-up loadout; two of them are parts of the sample recipes.
fn sample_parts() -> Vec<Part> {
    let part = |reference: &str, name: &str, kind: &str, size: i64, grade: &str, count: i64| Part {
        reference: reference.into(),
        name: name.into(),
        kind: kind.into(),
        size: Some(size),
        grade: versekit_core::jsonio::Value::str(grade),
        count,
    };
    vec![
        part("sample-cooler", "Frostline", "Cooler", 1, "C", 2),
        part("sample-plant", "Grand Core", "PowerPlant", 1, "C", 1),
        part("sample-gun", "Sample Repeater", "WeaponGun", 3, "A", 4),
    ]
}

/// The price of a part in the sample; the power plant has none.
fn sample_price(reference: &str) -> Option<f64> {
    match reference {
        "sample-cooler" => Some(22_700.0),
        "sample-gun" => Some(35_150.0),
        _ => None,
    }
}

impl Default for SalvageState {
    /// The sample: three ships, a made-up loadout, nothing fetched or written.
    fn default() -> Self {
        let entries: Vec<String> = SAMPLE_SHIPS.iter().map(|s| s.to_string()).collect();
        SalvageState {
            paths: None,
            ships: UexShips::default(),
            shops: Shops::default(),
            typed: String::new(),
            no_match: false,
            shown: Shown::Nothing,
            said: None,
            job: None,
            asking: None,
            ship_count: entries.len(),
            entries,
            parts_from: None,
            wants_part: false,
            list_overflow: 0.0,
            list_scrolled: 0.0,
        }
    }
}

impl SalvageState {
    /// With own data: the ships of UEX, the shop prices and the wrecks of the data folder.
    pub fn with_files(paths: &Paths) -> SalvageState {
        let ships = UexShips::load(&paths.app_file(versekit_core::ships::UEX_FILE));
        let entries = ships.choice_names();
        SalvageState {
            paths: Some(Paths::new(paths.host.clone())),
            shops: Shops::load(paths),
            ship_count: entries.len(),
            entries,
            ships,
            ..SalvageState::default()
        }
    }

    /// The name of the ship as the list writes it, or `None` for no ship of the list.
    fn ship_name(&self, typed: &str) -> Option<String> {
        if self.paths.is_none() {
            return SAMPLE_SHIPS.iter().find(|s| s.eq_ignore_ascii_case(typed)).map(|s| s.to_string());
        }
        if !self.ships.knows(typed) {
            return None;
        }
        Some(self.ships.name_of(typed).unwrap_or(typed).to_string())
    }

    fn price(&self, reference: &str) -> Option<f64> {
        if self.paths.is_none() {
            return sample_price(reference);
        }
        if !self.shops.known(reference) {
            return None;
        }
        self.shops.cheapest(reference).map(|(price, _, _)| price)
    }

    /// Takes the names of the parts into the list the field offers, when the game values changed.
    fn refresh_names(&mut self, crafting: &CraftingState) {
        let from = crafting
            .parts()
            .map(|(values, book, _)| (std::ptr::from_ref::<GameValues>(values) as usize, book.items.len()));
        if from == self.parts_from {
            return;
        }
        self.parts_from = from;
        self.entries.truncate(self.ship_count);
        if let Some((_, book, _)) = crafting.parts() {
            self.entries.extend(dismantle_view::part_names(book));
        }
    }

    /// The names of the parts in the list of the field.
    fn part_entries(&self) -> &[String] {
        &self.entries[self.ship_count..]
    }

    /// What `typed` names: a ship, a part (its own name, or the only entry that holds the text), or nothing
    /// clear. A ship wins over a part of the same name.
    fn target_of(&self, typed: &str) -> Target {
        let typed = typed.trim();
        if let Some(name) = self.ship_name(typed) {
            return Target::Ship(name);
        }
        if let Some(name) = dismantle_view::exact_name(self.part_entries(), typed) {
            return Target::Part(name.clone());
        }
        let wanted = lower(typed);
        let mut matching = self.entries.iter().enumerate().filter(|(_, name)| lower(name).contains(&wanted));
        match (matching.next(), matching.next()) {
            (Some((index, name)), None) if !wanted.is_empty() => {
                if index < self.ship_count {
                    Target::Ship(name.clone())
                } else {
                    Target::Part(name.clone())
                }
            }
            _ => Target::Unclear,
        }
    }

    /// Whether an entry of the list holds `typed` (the field says when nothing does).
    fn anything_holds(&self, typed: &str) -> bool {
        let wanted = lower(typed.trim());
        self.entries.iter().any(|name| lower(name).contains(&wanted))
    }

    /// "Look it up": what the field names is shown. A ship looked up before stands there at once, any other
    /// is fetched; a part shows its calculation.
    pub(crate) fn submit(&mut self, ctx: &egui::Context, crafting: &CraftingState) {
        let typed = self.typed.trim().to_string();
        match self.target_of(&typed) {
            Target::Ship(name) => self.look_up_ship(ctx, name),
            Target::Part(name) => self.open_part(&name),
            Target::Unclear => {
                // A product name or tag the list does not offer still leads to a recipe.
                let recipe = crafting.parts().is_some_and(|(values, book, _)| {
                    !typed.is_empty() && dismantle_view::has_result(values, book, &typed)
                });
                if recipe {
                    self.open_part(&typed);
                } else {
                    self.no_match = true;
                }
            }
        }
    }

    /// "Look it up" for the ship the field names; any other text is no match.
    #[cfg(test)]
    pub(crate) fn look_up(&mut self, ctx: &egui::Context) {
        match self.ship_name(self.typed.trim()) {
            Some(name) => self.look_up_ship(ctx, name),
            None => self.no_match = true,
        }
    }

    fn look_up_ship(&mut self, ctx: &egui::Context, name: String) {
        self.no_match = false;
        self.typed.clone_from(&name);
        if self.job.is_some() {
            return;
        }
        self.said = None;
        self.wants_part = false;
        let Some(paths) = &self.paths else {
            self.shown = Shown::Wreck { name, parts: sample_parts(), when: trade::now() };
            return;
        };
        let version = ErkulShips::load(&paths.app_file(versekit_core::ships::ERKUL_FILE)).game_version().to_string();
        if let Some(wreck) = salvage::remembered(paths, &name, &version) {
            self.shown = Shown::Wreck { name, parts: wreck.parts, when: wreck.when };
            return;
        }
        let maker = self.ships.manufacturer(&name);
        let paths = trade::job_paths(paths);
        let wanted = name.clone();
        let job = Job::spawn(ctx, "salvage-lookup", move || fetch(&paths, &wanted, &maker, &version));
        self.job = Some((name, job));
    }

    /// The calculation of a part opens; a jump from another page lands here.
    pub fn open_part(&mut self, name: &str) {
        self.no_match = false;
        self.said = None;
        self.wants_part = false;
        self.typed = name.to_string();
        self.shown = Shown::Part { name: name.to_string(), from: None };
    }

    /// A click on a part of the wreck: its calculation opens, the wreck stays the way back.
    fn open_part_of_wreck(&mut self, name: &str) {
        let wreck = std::mem::replace(&mut self.shown, Shown::Nothing);
        self.open_part(name);
        if matches!(wreck, Shown::Wreck { .. }) {
            self.shown = Shown::Part { name: name.to_string(), from: Some(Box::new(wreck)) };
        }
    }

    /// Whether the open calculation has a wreck to go back to.
    pub fn can_go_back(&self) -> bool {
        matches!(self.shown, Shown::Part { from: Some(_), .. })
    }

    /// Back from the calculation of a part to the wreck it was opened from.
    pub fn back_to_wreck(&mut self) {
        let Shown::Part { from: Some(wreck), .. } = &mut self.shown else { return };
        let wreck = std::mem::replace(wreck.as_mut(), Shown::Nothing);
        if let Some(name) = wreck.name() {
            self.typed = name.to_string();
        }
        self.shown = wreck;
    }

    /// A route that leads to the old page of the calculation: the field is the way in, the counters count
    /// the calculation until a result stands.
    pub fn focus_parts(&mut self) {
        if self.shown == Shown::Nothing {
            self.wants_part = true;
        }
    }

    /// The page id of the view that is open, for the usage counters: the calculation of a part, or the wreck.
    pub fn usage_view(&self) -> &'static str {
        match &self.shown {
            Shown::Part { .. } => VIEW_PART,
            Shown::Nothing if self.wants_part => VIEW_PART,
            _ => VIEW_WRECK,
        }
    }

    /// Whether a wreck or the calculation of a part is on show (or erkul could not be reached for a ship).
    pub fn showing_result(&self) -> bool {
        self.shown != Shown::Nothing
    }

    /// Takes in a lookup that ended.
    fn poll(&mut self) {
        let Some((_, job)) = &self.job else { return };
        let found = match job.poll() {
            Polled::Running => return,
            Polled::Done(found) => found,
            Polled::Lost => Err(Missing::Unreachable),
        };
        let Some((name, _)) = self.job.take() else { return };
        if let Some(paths) = &self.paths {
            self.shops = Shops::load(paths);
        }
        // The field names something else by now: the lookup stays kept, the page does not show it.
        if lower(self.typed.trim()) != lower(&name) {
            return;
        }
        self.shown = match found {
            Ok(wreck) => Shown::Wreck { name, parts: wreck.parts, when: wreck.when },
            // No code fits the name: the source does not list the ship, which the page says for no parts.
            Err(Missing::Unknown) => Shown::Wreck { name, parts: Vec::new(), when: trade::now() },
            Err(Missing::Unreachable) => Shown::Unreachable { name },
        };
    }

    /// Whether a lookup runs.
    pub fn looking(&self) -> bool {
        self.job.is_some()
    }

    /// How far the result sticks out at the right, for the edge check.
    pub fn list_overflow(&self) -> f32 {
        self.list_overflow
    }

    /// How far the result is scrolled down inside its own area.
    pub fn list_scrolled(&self) -> f32 {
        self.list_scrolled
    }

    /// "Clear input": the field, the line and the result.
    pub fn clear(&mut self) {
        self.typed.clear();
        self.no_match = false;
        self.said = None;
        self.wants_part = false;
        self.shown = Shown::Nothing;
    }

    /// A result stands only while the field names it: typing something else takes it away.
    fn drop_stale(&mut self) {
        if self.shown.name().is_some_and(|name| lower(name) != lower(self.typed.trim())) {
            self.shown = Shown::Nothing;
        }
    }

    /// The wrecks are forgotten: a wreck on show goes, the wreck behind a calculation goes with its way back.
    fn forget_shown(&mut self) {
        match &mut self.shown {
            Shown::Wreck { .. } | Shown::Unreachable { .. } => self.shown = Shown::Nothing,
            Shown::Part { from, .. } => *from = None,
            Shown::Nothing => {}
        }
    }
}

/// The lookup itself, in the thread of the job: the loadout from erkul, the missing shop prices, and the
/// wreck kept for next time; or why there is none.
fn fetch(paths: &Paths, name: &str, maker: &str, version: &str) -> Result<Wreck, Missing> {
    let web = Web::new(&trade::user_agent(), std::time::Duration::from_secs(ERKUL_SECONDS), ERKUL_LIMIT);
    let feed = FnFeed(|path: &str| {
        if cfg!(test) {
            return None;
        }
        web.get(&versekit_core::erkul_fetch::address(path)).ok()
    });
    let (_, parts) = salvage::look_up(&feed, name, maker)?;
    // The prices of the parts that no shop list knows yet, one after the other.
    let mut book = Shops::load(paths);
    let mut catalog = Catalog::load(paths);
    let missing: Vec<&Part> = parts.iter().filter(|p| !book.known(&p.reference)).collect();
    for part in missing {
        shops::fetch_part(
            paths,
            &mut book,
            &mut catalog,
            (&part.reference, &part.name),
            false,
            trade::now(),
            &trade::fetcher(),
        );
    }
    let wreck = Wreck { name: name.to_string(), parts, when: trade::now(), version: version.to_string() };
    if let Err(error) = salvage::remember(paths, wreck.clone()) {
        versekit_core::errors::record("salvage.save", &error);
    }
    Ok(wreck)
}

fn sub(scheme: &Scheme, text: impl Into<String>) -> RichText {
    RichText::new(text.into()).small().color(color(scheme.sub))
}

/// The kind of a part in words (`WeaponGun` is a ship weapon); a kind without a text stays as erkul writes it.
fn kind_word(kind: &str) -> String {
    let key = format!("s_wr_art_{}", kind.to_lowercase());
    if language::text_in(language::current(), &key).is_some() { t(&key) } else { kind.to_string() }
}

/// The warning before every number: a box with the symbol, the heading and the sentence, wrapped, never cut.
fn warning(ui: &mut Ui, scheme: &Scheme) {
    egui::Frame::new()
        .fill(color(scheme.surface))
        .stroke(egui::Stroke::new(1.0, color(scheme.gold)))
        .corner_radius(crate::control_radius(scheme))
        .inner_margin(egui::Margin { left: 12, right: 8, top: 6, bottom: 6 })
        .show(ui, |inner| {
            inner.set_width(inner.available_width());
            inner.horizontal_top(|row| {
                if let Some(image) = crate::icons::image("shield-alert", scheme.gold, 18.0) {
                    row.add(image);
                }
                row.vertical(|text| {
                    text.add(egui::Label::new(widgets::bold(text, t("s_wr_npc_kopf"), egui::TextStyle::Body)).wrap());
                    text.add(egui::Label::new(sub(scheme, t("s_wr_npc_warnung"))).wrap());
                });
            });
        });
}

/// Draws the page: the search stays on top, the result below scrolls.
pub fn show_page(ui: &mut Ui, scheme: &Scheme, state: &mut SalvageState, crafting: &mut CraftingState) {
    state.poll();
    // The game values are read in the background; the parts of the field and the calculation wait for them.
    crafting.tick(ui.ctx());
    state.refresh_names(crafting);
    ui.label(sub(scheme, t("s_zb_lead")));
    ui.add_space(6.0);
    // The search in its card: the field, what it says about the text, the buttons and what they answered.
    let look = widgets::card(ui, scheme, |ui| {
        widgets::card_title(ui, scheme, &t("s_zb_suche"));
        ui.label(sub(scheme, t("s_wr_such_hilfe")));
        let more = |n: usize| tf("s_mlg_mehr", &[("n", Arg::from(n))]);
        let hint = t("s_zb_suche_platz");
        let width = ui.available_width() - crate::layout::SAFETY;
        let entries = std::mem::take(&mut state.entries);
        let shown = widgets::SuggestField::new("bergung-suche", &hint, scheme, &more).width(width).searching().show(
            ui,
            &mut state.typed,
            &entries,
        );
        let ship_count = state.ship_count;
        state.entries = entries;
        let mut look = false;
        if let Some(index) = shown.picked
            && let Some(name) = state.entries.get(index).cloned()
        {
            // A pick is a choice: what was picked is shown at once.
            if index < ship_count {
                state.look_up_ship(ui.ctx(), name);
            } else {
                state.open_part(&name);
            }
        }
        if shown.response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) && !shown.is_open {
            look = true;
        }
        if shown.response.changed() {
            state.no_match = false;
        }
        state.drop_stale();
        let typed = state.typed.trim().to_string();
        // The fault stands at the field it belongs to.
        if state.no_match {
            ui.label(RichText::new(t("s_zb_kein_treffer")).color(color(scheme.red)));
        } else if !typed.is_empty() && state.shown.name().is_none() && !state.anything_holds(&typed) {
            ui.label(sub(scheme, t("s_zb_nichts_gefunden")));
        }
        ui.horizontal_wrapped(|row| {
            let word = t("s_wr_nachsehen");
            let busy = t("s_wr_hole");
            let button = widgets::Button::new(&word, ButtonKind::Strong, scheme)
                .blocked(state.looking().then_some(busy.as_str()));
            if row.add(button).clicked() && !state.looking() {
                look = true;
            }
            if row.add(widgets::Button::new(&t("s_wr_leeren"), ButtonKind::Normal, scheme)).clicked() {
                state.clear();
            }
            if state.paths.is_some()
                && row.add(widgets::Button::new(&t("s_wr_vergessen"), ButtonKind::Normal, scheme)).clicked()
            {
                let count = state.paths.as_ref().map_or(0, |p| salvage::load(p).len());
                if count == 0 {
                    state.said = Some((t("s_wr_nichts_gemerkt"), false));
                } else {
                    state.asking = Some(count);
                }
            }
            if state.can_go_back()
                && row.add(widgets::Button::new(&t("s_zb_zurueck_wrack"), ButtonKind::Normal, scheme)).clicked()
            {
                state.back_to_wreck();
            }
        });
        if let Some((said, fault)) = &state.said {
            let tone = if *fault { scheme.red } else { scheme.sub };
            ui.label(RichText::new(said).color(color(tone)));
        }
        look
    })
    .inner;
    if look {
        state.submit(ui.ctx(), crafting);
    }
    if state.asking.is_some() {
        ui.add_space(crate::layout::GAP);
        forget_question(ui, scheme, state);
    }
    ui.add_space(8.0);
    if state.looking() {
        widgets::loading(ui, scheme, &t("s_wr_hole"));
        return;
    }
    results(ui, scheme, state, crafting);
}

/// The question before the wrecks are forgotten, on the page.
fn forget_question(ui: &mut Ui, scheme: &Scheme, state: &mut SalvageState) {
    let Some(count) = state.asking else { return };
    let question =
        if count == 1 { t("s_wr_vergessen_frage_1") } else { tf("s_wr_vergessen_frage", &[("0", Arg::from(count))]) };
    widgets::card(ui, scheme, |card| {
        widgets::card_title(card, scheme, &t("s_wr_vergessen"));
        card.label(question);
        card.horizontal_wrapped(|row| {
            if row.add(widgets::Button::new(&t("s_wr_vergessen_ja"), ButtonKind::Danger, scheme)).clicked() {
                state.asking = None;
                state.forget_shown();
                let Some(paths) = &state.paths else { return };
                state.said = Some(match salvage::forget(paths) {
                    Ok(1) => (t("s_wr_vergessen_ok_1"), false),
                    Ok(n) => (tf("s_wr_vergessen_ok", &[("0", Arg::from(n))]), false),
                    Err(error) => {
                        versekit_core::errors::record("salvage.save", &error);
                        (t("m_b_nicht_gespeichert"), true)
                    }
                });
            }
            if row.add(widgets::Button::new(&t("e_abbrechen"), ButtonKind::Normal, scheme)).clicked() {
                state.asking = None;
            }
        });
    });
}

/// The result in a scroll area of its own: the wreck or the calculation of a part, or, before any, the
/// warning about player wrecks and the rules of the fabricator.
fn results(ui: &mut Ui, scheme: &Scheme, state: &mut SalvageState, crafting: &mut CraftingState) {
    let row_height = ui.text_style_height(&egui::TextStyle::Body) + 10.0;
    let room = crate::layout::list_room(ui, row_height);
    let area = egui::ScrollArea::vertical()
        .id_salt("bergung-ergebnis")
        .max_height(room)
        .min_scrolled_height(room)
        .auto_shrink([false, true]);
    let mut open = None;
    let shown = widgets::scroll_area(ui, scheme, area, |ui| {
        // The result stands in cards as wide as the cards above the area.
        ui.set_max_width(ui.available_width());
        open = result(ui, scheme, state, crafting);
    });
    state.list_overflow = trade::overflow_of(&shown);
    state.list_scrolled = shown.state.clamped_offset().y;
    if let Some(name) = open {
        state.open_part_of_wreck(&name);
    }
}

/// The result by what is shown; the name of a part of a wreck that was clicked.
fn result(ui: &mut Ui, scheme: &Scheme, state: &SalvageState, crafting: &mut CraftingState) -> Option<String> {
    match &state.shown {
        Shown::Nothing => {
            widgets::card(ui, scheme, |ui| {
                widgets::card_title(ui, scheme, &t("s_zb_vorab_kopf"));
                warning(ui, scheme);
                if let Some((values, _, _)) = crafting.parts() {
                    ui.add_space(6.0);
                    dismantle_view::rules(ui, scheme, values);
                }
            });
            None
        }
        Shown::Unreachable { name } => {
            widgets::card(ui, scheme, |ui| {
                widgets::card_title(ui, scheme, &tf("s_wr_ueberschrift", &[("0", Arg::from(name.as_str()))]));
                widgets::load_failed(ui, scheme, &t("s_wr_netz"), &t("s_wr_nochmal"));
            });
            None
        }
        Shown::Part { name, .. } => {
            dismantle_view::show(ui, scheme, crafting, name);
            None
        }
        Shown::Wreck { name, parts, when } => {
            widgets::card(ui, scheme, |ui| wreck(ui, scheme, state, crafting, (name, parts, *when))).inner
        }
    }
}

/// The card of the wreck: its title, the warning, the parts with their count, kind and price, the sum and what
/// it means. A part that has a recipe is a link to its calculation; the name of the one clicked comes back.
fn wreck(
    ui: &mut Ui,
    scheme: &Scheme,
    state: &SalvageState,
    crafting: &CraftingState,
    (name, parts, when): (&str, &[Part], f64),
) -> Option<String> {
    widgets::card_title(ui, scheme, &tf("s_wr_ueberschrift", &[("0", Arg::from(name))]));
    warning(ui, scheme);
    ui.add_space(8.0);
    if parts.is_empty() {
        let concept = state.paths.is_some() && state.ships.is_concept(name);
        let said = t(if concept { "s_wr_konzept" } else { "s_wr_unbekannt" });
        ui.add(egui::Label::new(RichText::new(said).color(color(scheme.gold))).wrap());
        return None;
    }
    ui.add(egui::Label::new(sub(scheme, t("s_wr_werk_hinweis"))).wrap());
    let lang = language::current();
    let (total, _, without) = salvage::value(parts, |code| state.price(code));
    ui.add_space(4.0);
    ui.label(
        widgets::bold(ui, tf("s_wr_wert", &[("0", Arg::from(trade::plain(lang, total)))]), egui::TextStyle::Body)
            .color(color(scheme.accent)),
    );
    ui.add(egui::Label::new(sub(scheme, t("s_wr_wert_hinweis"))).wrap());
    if without > 0 {
        let said =
            if without == 1 { t("s_wr_ohne_preis_1") } else { tf("s_wr_ohne_preis", &[("0", Arg::from(without))]) };
        ui.add(egui::Label::new(RichText::new(said).color(color(scheme.gold))).wrap());
    }
    ui.add_space(4.0);
    let linkable = |part: &Part| {
        crafting.parts().is_some_and(|(values, book, _)| dismantle_view::has_result(values, book, &part.name))
    };
    let mut open = None;
    widgets::list_frame(ui, scheme, |list| {
        list.set_width(list.available_width());
        for (k, part) in parts.iter().enumerate() {
            let frame = egui::Frame::new()
                .fill(if k % 2 == 1 { color(scheme.bar) } else { egui::Color32::TRANSPARENT })
                .inner_margin(egui::Margin::symmetric(6, 3));
            frame.show(list, |row| {
                row.set_width(row.available_width());
                let mut marks = vec![kind_word(&part.kind)];
                if let Some(size) = part.size {
                    marks.push(format!("S{size}{}", part.grade_text()));
                }
                let price = state.price(&part.reference).filter(|p| *p != 0.0);
                row.horizontal_wrapped(|line| {
                    line.label(
                        widgets::bold(line, tf("s_wr_stueck", &[("0", Arg::from(part.count))]), egui::TextStyle::Body)
                            .color(color(scheme.accent)),
                    );
                    if linkable(part) {
                        let link = line.add(egui::Link::new(RichText::new(&part.name).color(color(scheme.accent))));
                        if link
                            .on_hover_text(t("s_zb_zerlegen_hinweis"))
                            .on_hover_cursor(egui::CursorIcon::PointingHand)
                            .clicked()
                        {
                            open = Some(part.name.clone());
                        }
                    } else {
                        line.label(&part.name);
                    }
                    line.label(sub(scheme, marks.join("  ·  ")));
                    line.with_layout(egui::Layout::right_to_left(egui::Align::Center), |right| match price {
                        Some(price) => {
                            right.label(trade::auec(lang, price * part.count as f64));
                        }
                        None => {
                            right.label(sub(scheme, t("s_wr_kein_preis")));
                        }
                    });
                });
            });
        }
    });
    ui.add_space(4.0);
    let exact = versekit_core::formats::date_time(lang, when);
    widgets::data_age(ui, scheme, (trade::now() - when).max(0.0), &exact);
    open
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Event, Pos2, Rect, vec2};
    use versekit_core::language::{Choice, set_language};

    /// What one frame drew: the texts with their places, how far the page sticks out at the right, and the
    /// texts that were cut with "…".
    struct Drawn {
        texts: Vec<(String, Pos2)>,
        overflow: f32,
        cut: Vec<String>,
    }

    impl Drawn {
        fn has(&self, text: &str) -> bool {
            self.texts.iter().any(|(shown, _)| shown == text)
        }

        fn at(&self, text: &str) -> Pos2 {
            self.texts
                .iter()
                .filter(|(shown, _)| shown == text)
                .map(|(_, at)| *at)
                .min_by(|a, b| a.y.total_cmp(&b.y))
                .unwrap_or_else(|| panic!("{text:?} is not on the page: {:?}", self.texts))
        }
    }

    /// A window without a screen: the page in its own page frame, as the main window holds it.
    struct Frames {
        ctx: egui::Context,
        size: egui::Vec2,
    }

    impl Frames {
        fn new(width: f32, height: f32) -> Frames {
            let ctx = egui::Context::default();
            crate::install(&ctx);
            Frames { ctx, size: vec2(width, height) }
        }

        fn frame(&self, state: &mut SalvageState, crafting: &mut CraftingState, events: Vec<Event>) -> Drawn {
            fn collect(shape: &egui::epaint::Shape, drawn: &mut Drawn) {
                match shape {
                    egui::epaint::Shape::Text(text) => {
                        // Where the first letter stands: a wrapped label begins with the space of the row before it.
                        let first =
                            text.galley.rows.first().map_or(Pos2::ZERO, |row| row.rect_without_leading_space().min);
                        drawn.texts.push((text.galley.text().to_string(), text.pos + first.to_vec2()));
                        if text.galley.elided {
                            drawn.cut.push(text.galley.text().to_string());
                        }
                    }
                    egui::epaint::Shape::Vec(shapes) => shapes.iter().for_each(|shape| collect(shape, drawn)),
                    _ => {}
                }
            }
            let scheme = versekit_core::theme::current();
            let mut drawn = Drawn { texts: Vec::new(), overflow: 0.0, cut: Vec::new() };
            let mut output = self.ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, self.size)),
                    events,
                    ..Default::default()
                },
                |ui| {
                    drawn.overflow =
                        crate::layout::page(ui, scheme, "bergung-test", |ui| show_page(ui, scheme, state, crafting))
                            .overflow;
                },
            );
            output.textures_delta.clear();
            output.shapes.iter().for_each(|clipped| collect(&clipped.shape, &mut drawn));
            drawn
        }

        /// A few frames so that the page has settled.
        fn settle(&self, state: &mut SalvageState, crafting: &mut CraftingState) -> Drawn {
            let mut drawn = self.frame(state, crafting, Vec::new());
            for _ in 0..3 {
                drawn = self.frame(state, crafting, Vec::new());
            }
            drawn
        }

        /// A click as a person makes it: the pointer comes, presses, lets go.
        fn click(&self, state: &mut SalvageState, crafting: &mut CraftingState, text: &str) -> Drawn {
            let at = self.settle(state, crafting).at(text) + vec2(3.0, 3.0);
            let button = |pressed| Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            };
            self.frame(state, crafting, vec![Event::PointerMoved(at)]);
            self.frame(state, crafting, vec![button(true)]);
            self.frame(state, crafting, vec![button(false)]);
            self.settle(state, crafting)
        }
    }

    fn sample() -> (SalvageState, CraftingState) {
        (SalvageState::default(), CraftingState::default())
    }

    fn many_parts(count: i64) -> Vec<Part> {
        (0..count)
            .map(|n| Part {
                reference: format!("many-{n}"),
                name: format!("Many Part {n}"),
                kind: "Cooler".into(),
                size: Some(1),
                grade: versekit_core::jsonio::Value::str("C"),
                count: 1,
            })
            .collect()
    }

    #[test]
    fn the_one_field_finds_a_ship_and_a_part_and_says_when_it_finds_neither() {
        let _language = crate::language_lock();
        set_language(Choice::De);
        let (mut state, mut crafting) = sample();
        let frames = Frames::new(900.0, 900.0);
        frames.settle(&mut state, &mut crafting);
        let ctx = egui::Context::default();
        // A ship: its parts, named as the list writes the ship.
        state.typed = "cutlass black".into();
        state.submit(&ctx, &crafting);
        assert!(matches!(&state.shown, Shown::Wreck { name, .. } if name == "Cutlass Black"), "{:?}", state.shown);
        assert_eq!(state.typed, "Cutlass Black");
        let drawn = frames.settle(&mut state, &mut crafting);
        assert!(drawn.has(&tf("s_wr_ueberschrift", &[("0", Arg::from("Cutlass Black"))])), "{:?}", drawn.texts);
        assert!(!drawn.has(&tf("s_zl_dauer", &[("dauer", Arg::from(15))])));
        // A part: what the fabricator gives back, none of the wreck.
        state.typed = "frostline".into();
        state.submit(&ctx, &crafting);
        assert!(matches!(&state.shown, Shown::Part { name, from: None } if name == "Frostline"), "{:?}", state.shown);
        let drawn = frames.settle(&mut state, &mut crafting);
        assert!(drawn.has(&tf("s_zl_dauer", &[("dauer", Arg::from(15))])), "{:?}", drawn.texts);
        assert!(drawn.has("Frostline"), "{:?}", drawn.texts);
        let summed =
            drawn.has(&tf("s_zl_alles_zurueck", &[("prozent", Arg::from(50))])) || drawn.has(&t("s_zl_nichts"));
        assert!(summed, "{:?}", drawn.texts);
        assert!(!drawn.texts.iter().any(|(text, _)| text.starts_with("Ab Werk verbaut")), "{:?}", drawn.texts);
        // The only entry that holds the text is taken, a ship as well as a part.
        state.typed = "prospec".into();
        state.submit(&ctx, &crafting);
        assert!(matches!(&state.shown, Shown::Wreck { name, .. } if name == "Prospector"), "{:?}", state.shown);
        state.typed = "arc sh".into();
        state.submit(&ctx, &crafting);
        assert!(matches!(&state.shown, Shown::Part { name, .. } if name == "Arc Shield"), "{:?}", state.shown);
        // A product tag that the list does not offer still leads to its recipe.
        state.typed = "BP_CRAFT_COOL_AEGS_S02_Frostline".into();
        state.submit(&ctx, &crafting);
        assert!(matches!(&state.shown, Shown::Part { name, .. } if name == "BP_CRAFT_COOL_AEGS_S02_Frostline"));
        // Several entries or none: nothing opens, and the field says so.
        for typed in ["a", "Nonsense"] {
            state.clear();
            state.typed = typed.into();
            state.submit(&ctx, &crafting);
            assert!(state.no_match && state.shown == Shown::Nothing, "{typed}: {:?}", state.shown);
            let drawn = frames.settle(&mut state, &mut crafting);
            assert!(drawn.has(&t("s_zb_kein_treffer")), "{typed}: {:?}", drawn.texts);
        }
        // Typing a text that fits nothing says so at once, without a button.
        state.clear();
        state.typed = "Nonsense".into();
        let drawn = frames.settle(&mut state, &mut crafting);
        assert!(drawn.has(&t("s_zb_nichts_gefunden")), "{:?}", drawn.texts);
        state.typed = "Frost".into();
        assert!(!frames.settle(&mut state, &mut crafting).has(&t("s_zb_nichts_gefunden")));
    }

    #[test]
    fn a_name_that_is_a_ship_or_a_part_in_full_wins_over_longer_names_that_hold_it() {
        let _language = crate::language_lock();
        let (mut state, mut crafting) = sample();
        let frames = Frames::new(900.0, 900.0);
        frames.settle(&mut state, &mut crafting);
        state.entries.push("Prospector Armor".into());
        state.entries.push("Frostline Mk2".into());
        assert_eq!(state.target_of("prospector"), Target::Ship("Prospector".into()));
        assert_eq!(state.target_of("  FROSTLINE "), Target::Part("Frostline".into()));
        // Only the longer name holds the text twice over: no pick is made for the player.
        assert_eq!(state.target_of("Mk2"), Target::Part("Frostline Mk2".into()));
        assert_eq!(state.target_of("pro"), Target::Unclear, "a ship and a part hold it");
        assert_eq!(state.target_of(""), Target::Unclear);
    }

    #[test]
    fn the_part_offers_stand_in_the_list_of_the_field_after_the_ships() {
        let _language = crate::language_lock();
        let (mut state, mut crafting) = sample();
        let frames = Frames::new(900.0, 900.0);
        frames.settle(&mut state, &mut crafting);
        let ships: Vec<&str> = state.entries[..state.ship_count].iter().map(String::as_str).collect();
        assert_eq!(ships, SAMPLE_SHIPS);
        let parts: Vec<&str> = state.part_entries().iter().map(String::as_str).collect();
        assert_eq!(parts, ["Arc Shield", "Frostline", "Grand Core"]);
        // Drawing again does not add the parts a second time.
        frames.settle(&mut state, &mut crafting);
        assert_eq!(state.entries.len(), SAMPLE_SHIPS.len() + 3);
    }

    #[test]
    fn a_click_on_a_part_of_the_wreck_opens_its_calculation_and_the_way_back_returns() {
        let _language = crate::language_lock();
        set_language(Choice::De);
        let (mut state, mut crafting) = sample();
        let frames = Frames::new(900.0, 900.0);
        state.typed = "Cutlass Black".into();
        state.submit(&egui::Context::default(), &crafting);
        let wreck_title = tf("s_wr_ueberschrift", &[("0", Arg::from("Cutlass Black"))]);
        assert!(frames.settle(&mut state, &mut crafting).has(&wreck_title));
        assert!(!state.can_go_back());
        // A part without a recipe is no link: a click does nothing.
        let drawn = frames.click(&mut state, &mut crafting, "Sample Repeater");
        assert!(drawn.has(&wreck_title) && matches!(state.shown, Shown::Wreck { .. }), "{:?}", state.shown);
        // A part with a recipe: its calculation on the same page, the field names it, the wreck is the way back.
        let drawn = frames.click(&mut state, &mut crafting, "Frostline");
        assert!(
            matches!(&state.shown, Shown::Part { name, from: Some(_) } if name == "Frostline"),
            "{:?}",
            state.shown
        );
        assert_eq!(state.typed, "Frostline");
        assert!(state.can_go_back());
        assert_eq!(state.usage_view(), VIEW_PART);
        assert!(
            drawn.has(&tf("s_zl_dauer", &[("dauer", Arg::from(15))])) && !drawn.has(&wreck_title),
            "{:?}",
            drawn.texts
        );
        assert!(drawn.has(&t("s_zb_zurueck_wrack")), "the way back is a button on the page");
        // Back: the wreck stands there as it was, in the field and below it.
        let drawn = frames.click(&mut state, &mut crafting, &t("s_zb_zurueck_wrack"));
        assert!(
            matches!(&state.shown, Shown::Wreck { name, parts, .. } if name == "Cutlass Black" && parts.len() == 3)
        );
        assert_eq!(state.typed, "Cutlass Black");
        assert!(!state.can_go_back() && drawn.has(&wreck_title) && !drawn.has(&t("s_zb_zurueck_wrack")));
        assert_eq!(state.usage_view(), VIEW_WRECK);
        // The history step of the window does the same.
        frames.click(&mut state, &mut crafting, "Grand Core");
        assert!(state.can_go_back());
        state.back_to_wreck();
        assert!(matches!(state.shown, Shown::Wreck { .. }));
        // Typing something else in the calculation takes the way back away with the result.
        frames.click(&mut state, &mut crafting, "Frostline");
        state.typed = "Frostline 2".into();
        frames.settle(&mut state, &mut crafting);
        assert!(state.shown == Shown::Nothing && !state.can_go_back());
        // A part opened by a jump from another page has no wreck to go back to.
        state.open_part("Arc Shield");
        assert!(!state.can_go_back());
        let drawn = frames.settle(&mut state, &mut crafting);
        assert!(drawn.has("Arc Shield") && !drawn.has(&t("s_zb_zurueck_wrack")), "{:?}", drawn.texts);
    }

    #[test]
    fn the_result_stands_only_while_the_field_names_it() {
        let _language = crate::language_lock();
        set_language(Choice::De);
        let (mut state, mut crafting) = sample();
        let frames = Frames::new(900.0, 900.0);
        state.typed = "Prospector".into();
        state.submit(&egui::Context::default(), &crafting);
        frames.settle(&mut state, &mut crafting);
        assert!(matches!(state.shown, Shown::Wreck { .. }));
        // The name in another case is the same name.
        state.typed = "PROSPECTOR".into();
        frames.settle(&mut state, &mut crafting);
        assert!(matches!(state.shown, Shown::Wreck { .. }));
        state.typed = "Prospect".into();
        let drawn = frames.settle(&mut state, &mut crafting);
        assert_eq!(state.shown, Shown::Nothing);
        assert!(!drawn.has(&t("s_wr_werk_hinweis")), "{:?}", drawn.texts);
        // The same for a part, and "Clear input" empties the field and the result at once.
        state.open_part("Frostline");
        frames.settle(&mut state, &mut crafting);
        assert!(matches!(state.shown, Shown::Part { .. }));
        state.clear();
        assert!(state.typed.is_empty() && state.shown == Shown::Nothing);
        // The way of the usage counters: nothing shown counts as the wreck, unless a route asked for the part.
        assert_eq!(state.usage_view(), VIEW_WRECK);
        state.focus_parts();
        assert_eq!(state.usage_view(), VIEW_PART);
        state.look_up_ship(&egui::Context::default(), "Prospector".into());
        assert_eq!(state.usage_view(), VIEW_WRECK, "a result of the wreck ends the wish for the part");
        state.open_part("Frostline");
        assert_eq!(state.usage_view(), VIEW_PART);
        assert!(state.showing_result());
    }

    #[test]
    fn the_search_stays_on_top_while_only_the_result_scrolls() {
        let _language = crate::language_lock();
        set_language(Choice::De);
        let (mut state, mut crafting) = sample();
        let frames = Frames::new(900.0, 600.0);
        state.typed = "Cutlass Black".into();
        state.shown = Shown::Wreck { name: "Cutlass Black".into(), parts: many_parts(60), when: trade::now() };
        let before = frames.settle(&mut state, &mut crafting);
        // Rows outside the visible part of the result are not drawn at all.
        assert!(!before.has("Many Part 30"), "the window shows only the first rows");
        let (label, first) = (before.at(&t("s_zb_suche")), before.at("Many Part 0"));
        let wheel = Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: vec2(0.0, -400.0),
            phase: egui::TouchPhase::Move,
            source: egui::MouseWheelSource::Unknown,
            modifiers: egui::Modifiers::NONE,
        };
        let over = first + vec2(10.0, 5.0);
        frames.frame(&mut state, &mut crafting, vec![Event::PointerMoved(over)]);
        frames.frame(&mut state, &mut crafting, vec![wheel]);
        let mut after = frames.settle(&mut state, &mut crafting);
        for _ in 0..40 {
            after = frames.frame(&mut state, &mut crafting, Vec::new());
        }
        assert_eq!(after.at(&t("s_zb_suche")), label, "the field and its label do not move");
        assert_eq!(after.at(&t("s_wr_nachsehen")), before.at(&t("s_wr_nachsehen")), "nor the buttons");
        // The result scrolled inside its own area, and the page around it did not.
        assert!(state.list_scrolled() > 100.0, "scrolled by {}", state.list_scrolled());
        // The result moved: the first row is gone, rows from further down have come up.
        assert!(!after.has("Many Part 0"), "{:?}", after.texts);
        assert!(after.texts.iter().any(|(text, _)| text.starts_with("Many Part 1")), "{:?}", after.texts);
        let (second, last) = (before.at("Many Part 1").y, before.at("Many Part 5").y);
        assert!(last > second + 20.0, "counter-check of the measuring: rows stand below each other");
    }

    #[test]
    fn nothing_is_cut_off_in_any_view_at_the_narrowest_and_the_widest_size_in_both_languages() {
        let _language = crate::language_lock();
        let mut findings = Vec::new();
        for lang in [Choice::De, Choice::En] {
            set_language(lang);
            for points in [versekit_core::font_size::POINTS_RANGE.0, versekit_core::font_size::POINTS_RANGE.1] {
                for width in [380.0, 520.0, 900.0] {
                    for view in 0..7 {
                        let (mut state, mut crafting) = sample();
                        let frames = Frames::new(width, 3000.0);
                        crate::apply_size(&frames.ctx, points);
                        let wreck = Shown::Wreck { name: "Cutlass Black".into(), parts: sample_parts(), when: 1.0 };
                        match view {
                            0 => {}
                            1 => {
                                state.typed = "Cutlass Black".into();
                                state.shown = wreck;
                            }
                            2 => {
                                state.typed = "Frostline".into();
                                state.shown = Shown::Part { name: "Frostline".into(), from: Some(Box::new(wreck)) };
                            }
                            3 => state.open_part("Grand Core"),
                            4 => {
                                state.typed = "Nonsense".into();
                                state.no_match = true;
                            }
                            5 => {
                                state.typed = "Cutlass Black".into();
                                state.shown = Shown::Unreachable { name: "Cutlass Black".into() };
                            }
                            _ => {
                                state.asking = Some(3);
                                state.said = Some((t("s_wr_nichts_gemerkt"), false));
                            }
                        }
                        let drawn = frames.settle(&mut state, &mut crafting);
                        let at = format!("Ansicht {view} {lang:?} Größe {points} Breite {width}");
                        if drawn.overflow > 0.5 {
                            findings.push(format!("{at}: {:.0} zu breit", drawn.overflow));
                        }
                        if state.list_overflow() > 0.5 {
                            findings.push(format!("{at}: Ergebnis {:.0} zu breit", state.list_overflow()));
                        }
                        if !drawn.cut.is_empty() {
                            findings.push(format!("{at}: abgeschnitten {:?}", drawn.cut));
                        }
                    }
                }
            }
        }
        set_language(Choice::De);
        assert!(findings.is_empty(), "{}", findings.join("\n"));
    }

    #[test]
    fn forgetting_the_wrecks_takes_a_wreck_from_the_page_and_the_way_back_from_a_part() {
        let wreck = Shown::Wreck { name: "Cutlass Black".into(), parts: sample_parts(), when: 1.0 };
        let mut state = SalvageState { shown: wreck.clone(), ..SalvageState::default() };
        state.forget_shown();
        assert_eq!(state.shown, Shown::Nothing);
        state.shown = Shown::Part { name: "Frostline".into(), from: Some(Box::new(wreck)) };
        state.forget_shown();
        assert!(matches!(&state.shown, Shown::Part { name, from: None } if name == "Frostline"), "{:?}", state.shown);
        assert!(!state.can_go_back());
    }

    #[test]
    fn a_wreck_of_the_sample_shows_its_parts_their_value_and_what_has_no_price() {
        let _language = crate::language_lock();
        set_language(Choice::De);
        let (mut state, mut crafting) = sample();
        let frames = Frames::new(900.0, 900.0);
        let ctx = egui::Context::default();
        // A name that is no ship of the list fetches nothing and says so at the field.
        state.typed = "Nonsense".into();
        state.look_up(&ctx);
        assert!(state.no_match);
        assert_eq!(state.shown, Shown::Nothing);
        let drawn = frames.settle(&mut state, &mut crafting);
        assert!(drawn.has(&t("s_zb_kein_treffer")), "{:?}", drawn.texts);
        state.typed = "cutlass black".into();
        state.look_up(&ctx);
        let drawn = frames.settle(&mut state, &mut crafting);
        assert_eq!(drawn.overflow, 0.0);
        assert!(drawn.has(&t("s_wr_npc_kopf")) && drawn.has(&t("s_wr_npc_warnung")), "{:?}", drawn.texts);
        // The heading names the ship as the list writes it, not as it was typed.
        assert!(drawn.has(&tf("s_wr_ueberschrift", &[("0", Arg::from("Cutlass Black"))])), "{:?}", drawn.texts);
        // Two coolers and four guns have a price, the power plant has none.
        let total = trade::plain(language::current(), 2.0 * 22_700.0 + 4.0 * 35_150.0);
        assert!(drawn.has(&tf("s_wr_wert", &[("0", Arg::from(total))])), "{:?}", drawn.texts);
        assert!(drawn.has(&t("s_wr_ohne_preis_1")), "one piece, in the singular: {:?}", drawn.texts);
        let gun = format!("{}  ·  S3A", t("s_wr_art_weapongun"));
        assert!(drawn.has(&t("s_wr_kein_preis")) && drawn.has(&gun), "the kind in words: {:?}", drawn.texts);
        let age = tf("s_st_zuletzt", &[("0", Arg::from(""))]);
        assert!(drawn.texts.iter().any(|(text, _)| text.starts_with(age.trim_end())), "{:?}", drawn.texts);
        state.clear();
        let drawn = frames.settle(&mut state, &mut crafting);
        assert!(!drawn.has(&t("s_wr_werk_hinweis")), "{:?}", drawn.texts);
        // Before any result the page says what a player wreck is worth and what the fabricator keeps.
        assert!(drawn.has(&t("s_wr_npc_kopf")), "{:?}", drawn.texts);
        assert!(
            drawn.texts.iter().any(|(text, _)| text.starts_with("Fabricator: 50 % zurück")),
            "{:?}",
            drawn.texts
        );
    }

    #[test]
    fn each_part_of_the_sample_names_what_comes_back_and_what_is_lost() {
        let _language = crate::language_lock();
        set_language(Choice::De);
        let (mut state, mut crafting) = sample();
        let frames = Frames::new(900.0, 900.0);
        let names: Vec<String> = dismantle_view::part_names(crafting.parts().expect("sample values").1);
        assert_eq!(names.len(), 3);
        for name in names {
            state.open_part(&name);
            let drawn = frames.settle(&mut state, &mut crafting);
            assert_eq!(drawn.overflow, 0.0);
            assert!(drawn.has(&name), "{name}: {:?}", drawn.texts);
            assert!(drawn.has(&tf("s_zl_dauer", &[("dauer", Arg::from(15))])), "{:?}", drawn.texts);
            let summed = drawn.has(&tf("s_zl_alles_zurueck", &[("prozent", Arg::from(50))]))
                || drawn.texts.iter().any(|(text, _)| text.contains(&t("s_zl_nichts")));
            assert!(summed, "{name}: {:?}", drawn.texts);
            assert!(drawn.texts.iter().any(|(text, _)| text.starts_with("Fabricator: 50 % zurück")));
        }
        // A name without a recipe says so instead of showing an empty list.
        state.open_part("Nothing");
        let drawn = frames.settle(&mut state, &mut crafting);
        assert!(drawn.has(&tf("s_zl_kein_rezept", &[("name", Arg::from("Nothing"))])), "{:?}", drawn.texts);
    }

    #[test]
    fn a_ship_that_uex_lists_twice_is_offered_once() {
        use versekit_core::paths::Host;
        use versekit_core::pypath::Flavor;
        let dir = std::env::temp_dir().join(format!("versekit-bergung-doppelt-{}", versekit_core::testdir::run_tag()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("folder");
        let home = dir.to_string_lossy().into_owned();
        let paths = Paths::new(Host::new(Flavor::native(), &[(versekit_core::legacy::ENV_HOME, &home)], &home));
        // UEX gives the Sabre Raven EX under two ids with one name.
        let ships = r#"{"format": 5, "schiffe": {"290": {"name": "Aegis Sabre Raven EX"},
                        "291": {"name": "Aegis Sabre Raven EX"}, "3": {"name": "Aegis Sabre"}}}"#;
        std::fs::write(paths.app_file(versekit_core::ships::UEX_FILE), ships).expect("ships");
        let state = SalvageState::with_files(&paths);
        assert_eq!(state.entries, ["Aegis Sabre", "Aegis Sabre Raven EX"]);
        assert_eq!(state.ship_count, 2);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_wreck_looked_up_stands_there_again_without_the_net() {
        let _language = crate::language_lock();
        set_language(Choice::De);
        use versekit_core::paths::Host;
        use versekit_core::pypath::Flavor;
        let dir = std::env::temp_dir().join(format!("versekit-bergung-seite-{}", versekit_core::testdir::run_tag()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("folder");
        let home = dir.to_string_lossy().into_owned();
        let paths = Paths::new(Host::new(Flavor::native(), &[(versekit_core::legacy::ENV_HOME, &home)], &home));
        let ships = r#"{"format": 5, "schiffe": {"1": {"name": "Drake Cutlass Black", "werft": "Drake"},
                        "2": {"name": "Anvil Concept", "werft": "Anvil", "konzept": true}}}"#;
        std::fs::write(paths.app_file(versekit_core::ships::UEX_FILE), ships).expect("ships");
        salvage::remember(
            &paths,
            Wreck { name: "Drake Cutlass Black".into(), parts: sample_parts(), when: 1.0, version: String::new() },
        )
        .expect("kept");
        let mut state = SalvageState::with_files(&paths);
        let mut crafting = CraftingState::default();
        let frames = Frames::new(900.0, 900.0);
        let ctx = egui::Context::default();
        state.typed = "Cutlass Black".into();
        state.look_up(&ctx);
        assert!(!state.looking(), "kept: no lookup");
        assert!(matches!(&state.shown, Shown::Wreck { parts, .. } if parts.len() == 3));
        // Another ship is fetched; without the net nothing comes and the page says why.
        state.typed = "Anvil Concept".into();
        state.look_up(&ctx);
        let end = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while state.looking() && std::time::Instant::now() < end {
            state.poll();
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        // Unreachable is not "unknown ship": the page says erkul cannot be reached and how to try again.
        let drawn = frames.settle(&mut state, &mut crafting);
        assert!(drawn.has(&t("s_wr_netz")) && drawn.has(&t("s_wr_nochmal")), "{:?}", drawn.texts);
        assert!(!drawn.has(&t("s_wr_unbekannt")), "{:?}", drawn.texts);
        // A ship erkul does not list, which UEX calls a concept, is named as one.
        state.shown = Shown::Wreck { name: "Anvil Concept".into(), parts: Vec::new(), when: 1.0 };
        let drawn = frames.settle(&mut state, &mut crafting);
        assert!(drawn.has(&t("s_wr_konzept")), "{:?}", drawn.texts);
        // A lookup that ends after the field names something else is not shown.
        state.typed = "Cutlass Black".into();
        state.shown = Shown::Nothing;
        state.job = Some(("Anvil Concept".into(), Job::spawn(&ctx, "salvage-test", || Err(Missing::Unknown))));
        let end = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while state.looking() && std::time::Instant::now() < end {
            state.poll();
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert!(!state.looking() && state.shown == Shown::Nothing, "{:?}", state.shown);
        let _ = std::fs::remove_dir_all(dir);
    }
}
