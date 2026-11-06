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

//! The page "What to farm" (`pages._farm_list` of 4.12): which raw materials
//! are still missing to craft everything that is planned, the stock weighed
//! against all of it at once, where the materials lie, and below it the farm
//! notes with their counts.
//!
//! The planned items are the parts on "build" of the ships in the hangar and
//! on the wish list (their slots from the stored erkul files) and the farm
//! notes. Everything is added up across all items.

use std::collections::HashSet;
use std::time::SystemTime;

use egui::{RichText, Ui};
use versekit_core::erkul_slots::ErkulData;
use versekit_core::hangar;
use versekit_core::jsonio::{Map, Value};
use versekit_core::language::{Arg, t, tf};
use versekit_core::mining_data::{self, MiningData};
use versekit_core::notepad::{self, Note};
use versekit_core::paths::Paths;
use versekit_core::stock;
use versekit_core::stock_data::{StockData, lower};
use versekit_core::theme::Scheme;
use versekit_gamedata::craft_book::CraftBook;
use versekit_gamedata::farm::{self, FarmList, Need, Post};
use versekit_gamedata::values::GameValues;

use crate::color;
use crate::crafting_page::{CraftingState, fixed, g};
use crate::widgets;

const LOOK_SECONDS: f64 = 1.0;
/// The width of the count between minus and plus, so that the plus stays where it is.
const COUNT_WIDTH: f32 = 70.0;
/// So long a struck note can be taken back.
const UNDO_SECONDS: f64 = 10.0;
const NOTES_FILE: &str = "hangar.json";

/// What a click asks of the window.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    /// Open the crafting page with this item.
    ToCrafting(String),
    /// Show this material or place in mining.
    ToMining(String),
}

/// What the page holds besides the game values it borrows from the crafting page.
pub struct FarmState {
    paths: Option<Paths>,
    mining: MiningData,
    mining_seen: Option<SystemTime>,
    /// The slots of the ships, for the parts of the hangar and the wish list.
    erkul: ErkulData,
    erkul_seen: [Option<SystemTime>; 2],
    /// The whole fleet file: the ships, the wish list and the farm notes.
    notes: Map,
    notes_seen: Option<SystemTime>,
    stock: Vec<Value>,
    stock_seen: Option<SystemTime>,
    looked_at: f64,
    /// A write of the farm notes failed.
    save_failed: bool,
    /// The note struck last and when (`ui.input(|i| i.time)`), to take it back.
    struck: Option<(Note, f64)>,
}

impl Default for FarmState {
    /// The sample: made-up notes against a made-up stock.
    fn default() -> Self {
        let mut notes = hangar::empty();
        notepad::add(&mut notes, "Frostline", "", 2);
        notepad::add(&mut notes, "Arc Shield", "", 1);
        let stock = match versekit_core::jsonio::loads(
            r#"[{"material":"Iron","menge":0.7,"qualitaet":640,"ort":"Levski"},
                {"material":"Gold","menge":0.3,"qualitaet":800,"ort":"Levski"}]"#,
        ) {
            Ok(Value::Array(items)) => items,
            _ => Vec::new(),
        };
        FarmState {
            paths: None,
            mining: MiningData::default(),
            mining_seen: None,
            erkul: ErkulData::default(),
            erkul_seen: [None; 2],
            notes,
            notes_seen: None,
            stock,
            stock_seen: None,
            looked_at: f64::NEG_INFINITY,
            save_failed: false,
            struck: None,
        }
    }
}

fn file_time(file: &str) -> Option<SystemTime> {
    std::fs::metadata(file).and_then(|m| m.modified()).ok()
}

impl FarmState {
    /// With own data: the fleet file with the farm notes, the stock, the mining data and the slots of the
    /// ships of the data folder.
    pub fn with_files(paths: &Paths) -> FarmState {
        let mut state = FarmState {
            paths: Some(Paths::new(paths.host.clone())),
            notes: hangar::empty(),
            stock: Vec::new(),
            ..FarmState::default()
        };
        state.reload();
        state.reload_mining();
        state.reload_erkul();
        state
    }

    fn reload(&mut self) {
        let Some(paths) = &self.paths else { return };
        let (notes_file, stock_file) = (paths.app_file(NOTES_FILE), paths.app_file(stock::RAW_FILE));
        self.notes_seen = file_time(&notes_file);
        self.stock_seen = file_time(&stock_file);
        self.notes = hangar::read(&notes_file).unwrap_or_else(hangar::empty);
        self.stock = stock::load(&stock_file, "materials.load");
    }

    fn reload_mining(&mut self) {
        let Some(paths) = &self.paths else { return };
        let file = paths.app_file(mining_data::FILE);
        self.mining_seen = file_time(&file);
        self.mining = MiningData::load(&file);
    }

    fn reload_erkul(&mut self) {
        let Some(paths) = &self.paths else { return };
        self.erkul_seen = ErkulData::files(paths).map(|file| file_time(&file));
        self.erkul = ErkulData::load(paths);
    }

    /// Reads the files again when they changed on disk: the fleet file and the stock, the mining data (fetched
    /// after the start, the places come with it) and the slots of the ships.
    fn follow(&mut self, ui: &Ui) {
        let now = ui.input(|i| i.time);
        if now - self.looked_at < LOOK_SECONDS {
            return;
        }
        self.looked_at = now;
        let Some(paths) = &self.paths else { return };
        let changed = file_time(&paths.app_file(NOTES_FILE)) != self.notes_seen
            || file_time(&paths.app_file(stock::RAW_FILE)) != self.stock_seen;
        let mining_changed = file_time(&paths.app_file(mining_data::FILE)) != self.mining_seen;
        let erkul_changed = ErkulData::files(paths).map(|file| file_time(&file)) != self.erkul_seen;
        if changed {
            self.reload();
        }
        if mining_changed {
            self.reload_mining();
        }
        if erkul_changed {
            self.reload_erkul();
        }
    }

    /// Changes the farm notes and writes them: reads them again first, so
    /// that another place's change stays.
    fn edit(&mut self, change: impl FnOnce(&mut Map)) {
        let Some(paths) = &self.paths else {
            change(&mut self.notes);
            return;
        };
        let file = paths.app_file(NOTES_FILE);
        let _lock = hangar::lock();
        let mut held = hangar::load_held(&file);
        change(&mut held.data);
        self.save_failed = !hangar::save_held(&file, &mut held).ok();
        self.notes = held.data;
        self.notes_seen = file_time(&file);
    }
}

/// A quantity readable: whole numbers without a comma, small ones more exactly
/// (`0,64` is not `0,6`).
pub(crate) fn number(value: f64) -> String {
    if (value - value.round_ties_even()).abs() < 0.005 {
        return format!("{}", value.round_ties_even() as i64);
    }
    if value.abs() < 10.0 {
        let text = fixed(value, 2);
        let mark = if text.contains(',') { ',' } else { '.' };
        return if text.contains(mark) { text.trim_end_matches('0').trim_end_matches(mark).to_string() } else { text };
    }
    fixed(value, 1)
}

/// A quantity for the notes: four places at most, the decimal mark of the language.
fn amount_text(value: f64) -> String {
    g(stock::py_round(value, 4))
}

fn sub(scheme: &Scheme, text: impl Into<String>) -> RichText {
    RichText::new(text.into()).small().color(color(scheme.sub))
}

/// The share of a place as text: `25 %`, below one `< 1 %`.
pub(crate) fn share_text(share: f64) -> String {
    let percent = (share * 100.0).round_ties_even() as i64;
    if percent < 1 { t("s_bg_anteil_wenig") } else { tf("s_bg_anteil", &[("0", Arg::Int(percent))]) }
}

/// "Place (System) 25 % · Place 12 %", short enough for a line.
fn spots_text(spots: &[(String, String, f64)]) -> String {
    spots
        .iter()
        .map(|(place, system, share)| {
            if system.is_empty() {
                format!("{place} {}", share_text(*share))
            } else {
                format!("{place} ({system}) {}", share_text(*share))
            }
        })
        .collect::<Vec<_>>()
        .join(" · ")
}

/// The posts of the page, as the bill gives them: the parts on "build" of the
/// ships and the wish list and the notes that are to be crafted, not ticked
/// off, of which the recipe is known (one without counts as bought, as in the
/// shopping list).
fn posts(state: &FarmState, crafting: &CraftingState) -> Vec<Post> {
    let Some((values, book, _)) = crafting.parts() else { return Vec::new() };
    let slots = |ship: &Map, chosen: &Map| state.erkul.cart_slots(ship, chosen);
    farm::planned_posts(&state.notes, values, book, Some(&slots))
}

/// Draws the page; what a click asks of the window comes back.
pub fn show_page(ui: &mut Ui, scheme: &Scheme, state: &mut FarmState, crafting: &mut CraftingState) -> Option<Action> {
    state.follow(ui);
    // The values come with the window, not with a visit to the crafting page.
    crafting.values_or_wait(ui, scheme)?;
    let (values, book, data) = crafting.parts()?;
    let notes = notepad::notes(&state.notes);
    let planned = posts(state, crafting);
    let list = farm::farm_list(values, book, &planned, &state.stock, data);
    let mut action = None;
    // The places of the missing materials, for their rows and for the card of the gathering places.
    let found = (list.posts > 0 && !list.missing.is_empty()).then(|| {
        let names: Vec<String> = list.missing.iter().map(|n| n.material.clone()).collect();
        farm::locations(&names, &state.mining)
    });
    let head = match list.posts {
        0 => t("s_fl_bedarf_kopf"),
        1 => t("s_p9_kopf_eins"),
        posts => tf("s_fl_kopf", &[("n", Arg::from(posts))]),
    };
    // The first card: what is missing for the plan, with the notices of the page above it.
    let empty = widgets::card(ui, scheme, |ui| {
        widgets::card_title(ui, scheme, &head);
        if state.save_failed {
            ui.label(RichText::new(t("s_bp_speichern_fehler")).color(color(scheme.red)));
        }
        // The note struck last can be taken back for a few seconds.
        let now = ui.input(|i| i.time);
        if state.struck.as_ref().is_some_and(|(_, at)| now - at > UNDO_SECONDS) {
            state.struck = None;
        }
        if let Some((note, _)) = state.struck.clone() {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(500));
            ui.horizontal_wrapped(|row| {
                row.label(sub(scheme, tf("s_p9_gestrichen", &[("0", Arg::from(note.name.as_str()))])));
                if row.add(egui::Link::new(RichText::new(t("s_p9_rueckgaengig")).small())).clicked() {
                    state.edit(|data| {
                        notepad::restore(data, &note);
                    });
                    state.struck = None;
                }
            });
        }
        if notes.is_empty() && planned.is_empty() {
            // One empty page with one way: the button of the crafting page.
            widgets::empty_state(ui, scheme, "diamond", &t("s_fl_nichts_geplant"));
            return true;
        }
        if list.posts == 0 {
            // Notes are there, but none is to be crafted (bought, ticked off) or has a recipe.
            if list.without_recipe.is_empty() {
                ui.label(sub(scheme, t("s_p9_alles_gekauft")));
            }
        } else {
            ui.add_space(4.0);
            match &found {
                Some(found) => missing(ui, scheme, &list, found, data, &mut action),
                None => {
                    ui.label(RichText::new(t("s_fl_alles_da")).color(color(scheme.accent)));
                }
            }
        }
        if !list.without_recipe.is_empty() {
            let names = list.without_recipe.join(", ");
            let key = if list.without_recipe.len() == 1 { "s_p9_ohne_rezept_eins" } else { "s_fl_ohne_rezept" };
            let said = tf(key, &[("n", Arg::from(list.without_recipe.len())), ("teile", Arg::from(names))]);
            ui.label(RichText::new(said).small().color(color(scheme.gold)));
        }
        false
    })
    .inner;
    if empty {
        return None;
    }
    ui.add_space(crate::layout::GAP);
    if let Some(found) = found.as_ref().filter(|found| !found.gathering.is_empty()) {
        widgets::card(ui, scheme, |ui| gathering(ui, scheme, found, &mut action));
        ui.add_space(crate::layout::GAP);
    }
    if list.posts > 0 && !list.complete.is_empty() {
        widgets::card(ui, scheme, |ui| {
            widgets::card_title(ui, scheme, &tf("s_fl_reicht_kopf", &[("n", Arg::from(list.complete.len()))]));
            for (k, need) in list.complete.iter().enumerate() {
                need_row(ui, scheme, need, k, false, data, &mut action);
            }
        });
        ui.add_space(crate::layout::GAP);
    }
    if let Some(a) = widgets::card(ui, scheme, |ui| notes_block(ui, scheme, state, crafting, &notes, &list)).inner {
        action = Some(a);
    }
    action
}

/// A name that leads somewhere: an accent link with the hand and a focus ring, like the other pages.
fn link(ui: &mut Ui, text: RichText, hover: &str) -> egui::Response {
    ui.add(egui::Link::new(text)).on_hover_text(hover)
}

/// The rows of the missing materials, each with the places where it is found.
fn missing(
    ui: &mut Ui,
    scheme: &Scheme,
    list: &FarmList,
    found: &farm::Locations,
    data: &StockData,
    action: &mut Option<Action>,
) {
    for (k, need) in list.missing.iter().enumerate() {
        need_row(ui, scheme, need, k, true, data, action);
        if let Some(spots) = found.per_ore.get(&need.material).filter(|s| !s.is_empty()) {
            let line = tf("s_fl_fundorte", &[("0", Arg::from(spots_text(spots)))]);
            ui.horizontal_wrapped(|row| {
                row.add_space(12.0);
                if link(row, RichText::new(line).small(), &t("s_fl_zum_bergbau")).clicked() {
                    *action = Some(Action::ToMining(need.material.clone()));
                }
            });
        }
    }
}

/// The card of the places that hold several of the missing materials.
fn gathering(ui: &mut Ui, scheme: &Scheme, found: &farm::Locations, action: &mut Option<Action>) {
    widgets::card_title(ui, scheme, &t("s_fl_sammel_kopf")).on_hover_text(t("s_fl_sammel_hilfe"));
    for place in &found.gathering {
        let title =
            if place.system.is_empty() { place.name.clone() } else { format!("{} ({})", place.name, place.system) };
        let text =
            tf("s_fl_sammel_zeile", &[("n", Arg::from(place.ores.len())), ("erze", Arg::from(place.ores.join(", ")))]);
        if link(ui, RichText::new(format!("{title}   {text}")).small(), &t("s_fl_zum_bergbau")).clicked() {
            *action = Some(Action::ToMining(place.name.clone()));
        }
    }
}

/// An amount with its unit after it: SCU, or pieces for goods counted in pieces.
fn with_unit(value: f64, piece: bool) -> String {
    format!("{} {}", number(value), if piece { t("s_lg_stueck") } else { "SCU".to_string() })
}

fn need_row(
    ui: &mut Ui,
    scheme: &Scheme,
    need: &Need,
    k: usize,
    short: bool,
    data: &StockData,
    action: &mut Option<Action>,
) {
    let piece = data.is_piece(&need.material);
    let frame = egui::Frame::new()
        .fill(if k % 2 == 1 { color(scheme.bar) } else { egui::Color32::TRANSPARENT })
        .inner_margin(egui::Margin::symmetric(6, 3));
    frame.show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal_wrapped(|row| {
            let name = RichText::new(&need.material).color(color(if short { scheme.accent } else { scheme.sub }));
            if link(row, name, &t("s_fl_zum_bergbau")).clicked() {
                *action = Some(Action::ToMining(need.material.clone()));
            }
            if short {
                let stand = tf(
                    "s_fl_stand",
                    &[
                        ("braucht", Arg::from(with_unit(need.needed, piece))),
                        ("hat", Arg::from(with_unit(need.have, piece))),
                    ],
                );
                row.label(sub(scheme, stand));
                row.label(
                    RichText::new(tf("s_fl_fehlt", &[("menge", Arg::from(with_unit(need.missing, piece)))]))
                        .color(color(scheme.gold)),
                );
            } else {
                row.label(RichText::new(t("s_fl_genug")).small().color(color(scheme.accent)));
            }
            if need.too_low > 0.0 {
                let text = tf(
                    "s_fl_zu_gering",
                    &[
                        ("menge", Arg::from(with_unit(need.too_low, piece))),
                        ("guete", Arg::from(number(need.min_quality))),
                    ],
                );
                row.label(sub(scheme, text));
            }
        });
    });
}

/// A button that stands for an action on one item: its name stands in the name for a screen reader.
pub(crate) fn row_button(ui: &mut Ui, scheme: &Scheme, icon: &str, meaning: &str, item: &str, enabled: bool) -> bool {
    let response = ui.add_enabled_ui(enabled, |ui| widgets::icon_button(ui, scheme, icon, meaning)).inner;
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::accesskit::Role::Button, enabled, format!("{item}: {meaning}"))
    });
    if !enabled {
        // egui shows no tooltip on a disabled widget by itself: the reason has to be asked for.
        response.clone().on_disabled_hover_text(meaning);
    }
    enabled && response.clicked()
}

/// The kind of a note as the blueprint list names it and the size of its product, from the craft book; `None`
/// for a note the book does not know.
fn kind_and_size(values: &GameValues, book: &CraftBook, note: &Note) -> Option<(String, Option<i64>)> {
    let blueprint = farm::blueprint_of(book, &note.reference, &note.name);
    let item = book.items.iter().find(|item| item.basis == blueprint || item.name == note.name)?;
    let size = values.recipes.get(item.recipe).and_then(|recipe| values.product(recipe)).and_then(|p| p.size);
    let title = if item.catalog_kind.is_empty() {
        String::new()
    } else {
        versekit_core::blueprints::kind_readable(&item.catalog_kind)
    };
    Some((title, size))
}

/// The notes of one kind, each with its size.
type SizedNotes<'a> = Vec<(Option<i64>, &'a Note)>;

/// The notes in groups by kind, the kinds alphabetical, within a kind by size, then by name (`size_order`). Notes
/// without a known kind stand last, in a group without a title.
fn note_groups<'a>(
    notes: &'a [Note],
    kind_size: impl Fn(&Note) -> Option<(String, Option<i64>)>,
) -> Vec<(String, Vec<&'a Note>)> {
    let mut groups: Vec<(String, SizedNotes<'a>)> = Vec::new();
    for note in notes {
        let (title, size) = kind_size(note).unwrap_or_default();
        match groups.iter_mut().find(|(known, _)| *known == title) {
            Some((_, rows)) => rows.push((size, note)),
            None => groups.push((title, vec![(size, note)])),
        }
    }
    groups.sort_by_cached_key(|(title, _)| (title.is_empty(), title.to_lowercase()));
    groups
        .into_iter()
        .map(|(title, mut rows)| {
            rows.sort_by_cached_key(|(size, note)| versekit_core::size_order::key(*size, &note.name));
            (title, rows.into_iter().map(|(_, note)| note).collect())
        })
        .collect()
}

fn notes_block(
    ui: &mut Ui,
    scheme: &Scheme,
    state: &mut FarmState,
    crafting: &CraftingState,
    notes: &[Note],
    list: &FarmList,
) -> Option<Action> {
    let mut action = None;
    widgets::card_title(ui, scheme, &t("s_mz_titel"));
    if notes.is_empty() {
        // Only parts of the ships are planned: the sentence says where the button for the notes is.
        ui.add(egui::Label::new(sub(scheme, t("s_mz_leer"))).wrap());
        return None;
    }
    // The material of an item stands "needs"; whether it is missing is told by
    // the colour, and that comes from the same calculation as the sums above.
    let short: HashSet<String> = list.missing.iter().map(|n| lower(n.material.trim())).collect();
    let mut set_count: Option<(String, i64)> = None;
    let mut strike: Option<Note> = None;
    let groups = match crafting.parts() {
        Some((values, book, _)) => note_groups(notes, |note| kind_and_size(values, book, note)),
        None => note_groups(notes, |_| None),
    };
    for (title, group) in &groups {
        // A small heading per kind, as in the blueprint list.
        if !title.is_empty() {
            ui.add_space(6.0);
            ui.label(RichText::new(title).small().strong().color(color(scheme.sub)));
        }
        notes_rows(ui, scheme, crafting, group, &short, &mut set_count, &mut strike, &mut action);
    }
    if let Some((name, count)) = set_count {
        state.edit(|data| {
            notepad::set_count(data, &name, count);
        });
    }
    if let Some(note) = strike {
        state.edit(|data| {
            notepad::remove(data, &note.name);
        });
        state.struck = Some((note, ui.input(|i| i.time)));
    }
    action
}

/// The rows of one group of notes, striped from its first row.
#[allow(clippy::too_many_arguments)]
fn notes_rows(
    ui: &mut Ui,
    scheme: &Scheme,
    crafting: &CraftingState,
    notes: &[&Note],
    short: &HashSet<String>,
    set_count: &mut Option<(String, i64)>,
    strike: &mut Option<Note>,
    action: &mut Option<Action>,
) {
    for (k, note) in notes.iter().copied().enumerate() {
        let frame = egui::Frame::new()
            .fill(if k % 2 == 1 { color(scheme.bar) } else { egui::Color32::TRANSPARENT })
            .inner_margin(egui::Margin::symmetric(6, 3));
        frame.show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_wrapped(|row| {
                let less = t(if note.count > 1 { "s_mz_weniger" } else { "s_mz_weniger_aus" });
                if row_button(row, scheme, "minus", &less, &note.name, note.count > 1) {
                    *set_count = Some((note.name.clone(), note.count - 1));
                }
                // A fixed width, so the plus does not wander with the digits.
                row.add_sized(
                    [COUNT_WIDTH, row.spacing().interact_size.y],
                    egui::Label::new(format!("{} {}", note.count, t("s_mz_stueck"))),
                );
                if row_button(row, scheme, "plus", &t("s_mz_mehr"), &note.name, true) {
                    *set_count = Some((note.name.clone(), note.count + 1));
                }
                let name = RichText::new(&note.name).color(color(scheme.accent));
                if link(row, name, &t("s_mz_zur_herstellung")).clicked() {
                    *action = Some(Action::ToCrafting(note.name.clone()));
                }
                // The action on the row stands at the right edge, in the colour of what it does.
                row.with_layout(egui::Layout::right_to_left(egui::Align::Center), |right| {
                    if row_button(right, scheme, "trash-2", &t("s_mz_weg"), &note.name, true) {
                        *strike = Some(note.clone());
                    }
                });
            });
            note_material(ui, scheme, crafting, note, short);
        });
    }
}

/// What one item on the notes needs, by its recipe.
fn note_material(ui: &mut Ui, scheme: &Scheme, crafting: &CraftingState, note: &Note, short: &HashSet<String>) {
    let Some((values, book, _)) = crafting.parts() else { return };
    let post = Post { name: note.name.clone(), reference: note.reference.clone(), pieces: note.count };
    let Some(recipe) = farm::recipe_of(values, book, &post) else {
        ui.label(sub(scheme, t("s_mz_kein_rezept")));
        return;
    };
    ui.horizontal_wrapped(|row| {
        row.add_space(12.0);
        for tier in versekit_gamedata::craft_book::ingredients(recipe) {
            for ingredient in tier {
                let needed = ingredient.amount * note.count as f64;
                let lacks = short.contains(&lower(ingredient.material.trim()));
                let tone = if lacks { scheme.gold } else { scheme.accent };
                let text = format!("{} {}", ingredient.material, amount_text(needed));
                row.label(RichText::new(tf("s_mz_braucht", &[("0", Arg::from(text))])).small().color(color(tone)));
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Rect, vec2};
    use versekit_core::language::{Choice, set_language};

    /// A mining file with two ores that lie at two places together.
    const MINING: &str = r#"{"format": 1, "locations": [
      {"locationName": "Daymar", "system": "Stanton", "groups": [{"groupName": "SpaceShip_Mineables", "deposits": [{"compositionGuid": "c", "relativeProbability": 1}]}]},
      {"locationName": "Cellin", "system": "Stanton", "groups": [{"groupName": "SpaceShip_Mineables", "deposits": [{"compositionGuid": "d", "relativeProbability": 1}]}]}],
     "compositions": {"c": {"parts": [{"elementName": "Iron", "probability": 1, "minPercent": 50, "maxPercent": 50}, {"elementName": "Gold", "probability": 1, "minPercent": 50, "maxPercent": 50}]},
                      "d": {"parts": [{"elementName": "Iron", "probability": 1, "minPercent": 100, "maxPercent": 100}]}}}"#;

    fn texts(shapes: &[egui::epaint::ClippedShape]) -> Vec<String> {
        fn collect(shape: &egui::epaint::Shape, out: &mut Vec<String>) {
            match shape {
                egui::epaint::Shape::Text(text) => out.push(text.galley.text().to_string()),
                egui::epaint::Shape::Vec(shapes) => shapes.iter().for_each(|shape| collect(shape, out)),
                _ => {}
            }
        }
        let mut out = Vec::new();
        shapes.iter().for_each(|clipped| collect(&clipped.shape, &mut out));
        out
    }

    fn context() -> egui::Context {
        let ctx = egui::Context::default();
        crate::install(&ctx);
        ctx
    }

    /// One frame inside the page frame of the window: the texts and how far the page sticks out.
    fn draw(
        ctx: &egui::Context,
        state: &mut FarmState,
        crafting: &mut CraftingState,
        width: f32,
    ) -> (Vec<String>, f32) {
        let scheme = versekit_core::theme::current();
        let mut over = 0.0;
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(width, 1800.0))),
                ..Default::default()
            },
            |ui| {
                let shown = crate::layout::page(ui, scheme, "farm-test", |ui| show_page(ui, scheme, state, crafting));
                over = shown.overflow;
            },
        );
        output.textures_delta.clear();
        (texts(&output.shapes), over)
    }

    fn mining() -> MiningData {
        MiningData::of(&versekit_core::jsonio::loads(MINING).expect("json"))
    }

    #[test]
    fn quantities_read_whole_and_small_ones_exactly() {
        let _language = crate::language_lock();
        set_language(Choice::De);
        assert_eq!(number(4.0), "4");
        assert_eq!(number(0.64), "0,64");
        assert_eq!(number(0.5), "0,5");
        assert_eq!(number(4.0 + 0.4), "4,4");
        assert_eq!(number(12.34), "12,3");
        assert_eq!(number(3.999), "4");
        // The decimal mark follows the language.
        set_language(Choice::En);
        assert_eq!(number(0.64), "0.64");
        assert_eq!(number(12.34), "12.3");
        assert_eq!(amount_text(0.5), "0.5");
        set_language(Choice::De);
    }

    #[test]
    fn the_notes_show_their_material_and_a_count_change_is_written() {
        let _language = crate::language_lock();
        set_language(Choice::De);
        let mut crafting = CraftingState::default();
        let mut state = FarmState::default();
        let (shown, over) = draw(&context(), &mut state, &mut crafting, 900.0);
        assert_eq!(over, 0.0);
        assert!(shown.contains(&tf("s_fl_kopf", &[("n", Arg::Int(3))])), "{shown:?}");
        assert!(shown.iter().any(|s| s.starts_with("Iron") && s.contains("fehlt") || s == "Iron"), "{shown:?}");
        assert!(shown.contains(&t("s_mz_titel")));
        state.edit(|data| {
            notepad::set_count(data, "Frostline", 5);
        });
        assert_eq!(notepad::count_of_item(&state.notes, "Frostline"), 5);
    }

    #[test]
    fn places_gathering_places_and_the_covered_group_are_drawn_in_both_languages() {
        let _language = crate::language_lock();
        let mut crafting = CraftingState::default();
        let mut state = FarmState { mining: mining(), ..FarmState::default() };
        // Iron and Gold are missing (the sample stock is small); the stock covers nothing of Agricium.
        for lang in [Choice::De, Choice::En] {
            set_language(lang);
            let (shown, over) = draw(&context(), &mut state, &mut crafting, 900.0);
            assert_eq!(over, 0.0, "{lang:?}");
            assert!(shown.contains(&t("s_fl_sammel_kopf")), "{lang:?}: {shown:?}");
            assert!(shown.iter().any(|s| s.contains("Daymar (Stanton)")), "{lang:?}: {shown:?}");
            assert!(shown.iter().any(|s| s.starts_with(&t("s_fl_fundorte").replace("{0}", ""))), "{lang:?}");
            assert!(shown.iter().any(|s| s.contains("SCU")), "units follow the numbers: {shown:?}");
        }
        set_language(Choice::De);
    }

    #[test]
    fn the_empty_page_has_one_way_and_a_struck_note_can_be_taken_back() {
        let _language = crate::language_lock();
        set_language(Choice::De);
        let mut crafting = CraftingState::default();
        let mut state = FarmState::default();
        let names: Vec<String> = notepad::notes(&state.notes).iter().map(|n| n.name.clone()).collect();
        for name in &names {
            state.edit(|data| {
                notepad::remove(data, name);
            });
        }
        let (shown, _) = draw(&context(), &mut state, &mut crafting, 900.0);
        assert!(shown.contains(&t("s_fl_nichts_geplant")), "{shown:?}");
        assert!(!shown.contains(&t("s_mz_leer")), "two instructions that differ are one too many");
        // Striking keeps the note for the undo.
        state.edit(|data| {
            notepad::add(data, "Frostline", "", 2);
        });
        let note = notepad::notes(&state.notes).remove(0);
        state.edit(|data| {
            notepad::remove(data, &note.name);
        });
        state.struck = Some((note, 0.0));
        let (shown, _) = draw(&context(), &mut state, &mut crafting, 900.0);
        assert!(shown.contains(&t("s_p9_rueckgaengig")), "{shown:?}");
        // After a few seconds the offer is gone.
        let ctx = context();
        let scheme = versekit_core::theme::current();
        let mut output = ctx.run_ui(
            egui::RawInput {
                time: Some(UNDO_SECONDS + 5.0),
                screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(900.0, 900.0))),
                ..Default::default()
            },
            |ui| {
                show_page(ui, scheme, &mut state, &mut crafting);
            },
        );
        output.textures_delta.clear();
        assert!(!texts(&output.shapes).contains(&t("s_p9_rueckgaengig")));
        assert!(state.struck.is_none());
    }

    /// The undo puts the whole note back, bought or ticked off as it was.
    #[test]
    fn the_undo_restores_way_and_tick() {
        let mut state = FarmState::default();
        state.edit(|data| {
            notepad::add(data, "Helm", "", 2);
        });
        let mut note = notepad::notes(&state.notes).into_iter().find(|n| n.name == "Helm").expect("note");
        note.way = "kaufen".into();
        note.done = true;
        state.edit(|data| {
            notepad::remove(data, "Helm");
            notepad::restore(data, &note);
        });
        let back = notepad::notes(&state.notes).into_iter().find(|n| n.name == "Helm").expect("back");
        assert_eq!((back.way.as_str(), back.done, back.count), ("kaufen", true, 2));
    }

    fn note(name: &str) -> Note {
        Note { name: name.into(), reference: String::new(), count: 1, way: "bauen".into(), done: false }
    }

    #[test]
    fn the_notes_go_by_kind_then_size_and_one_without_a_kind_last() {
        // As the notes come: by name.
        let notes: Vec<Note> = [
            "Arctic Cooler",
            "Omnisky III Cannon",
            "Omnisky IX Cannon",
            "Omnisky VI Cannon",
            "Pembroke Helmet",
            "Unbekanntes Teil",
            "Wave Cooler",
        ]
        .into_iter()
        .map(note)
        .collect();
        let kind_size = |note: &Note| -> Option<(String, Option<i64>)> {
            let (kind, size) = match note.name.as_str() {
                "Arctic Cooler" => ("Kühler", Some(2)),
                "Wave Cooler" => ("Kühler", Some(1)),
                "Omnisky III Cannon" => ("Schiffswaffe", Some(1)),
                "Omnisky VI Cannon" => ("Schiffswaffe", Some(2)),
                "Omnisky IX Cannon" => ("Schiffswaffe", Some(3)),
                "Pembroke Helmet" => ("Helm", None),
                _ => return None,
            };
            Some((kind.to_string(), size))
        };
        let shown: Vec<(String, Vec<&str>)> = note_groups(&notes, kind_size)
            .into_iter()
            .map(|(title, rows)| (title, rows.iter().map(|n| n.name.as_str()).collect()))
            .collect();
        let expected: Vec<(String, Vec<&str>)> = vec![
            ("Helm".into(), vec!["Pembroke Helmet"]),
            ("Kühler".into(), vec!["Wave Cooler", "Arctic Cooler"]),
            ("Schiffswaffe".into(), vec!["Omnisky III Cannon", "Omnisky VI Cannon", "Omnisky IX Cannon"]),
            (String::new(), vec!["Unbekanntes Teil"]),
        ];
        assert_eq!(shown, expected, "kinds alphabetical, by size within, no kind last");
    }

    #[test]
    fn the_kind_and_the_size_of_a_note_come_from_the_craft_book() {
        use versekit_gamedata::craft_book::CraftItem;
        use versekit_gamedata::values::{ItemValues, Recipe};
        let item = CraftItem {
            basis: "Omnisky VI Cannon".into(),
            name: "Omnisky VI Cannon".into(),
            manufacturer: String::new(),
            kind: String::new(),
            subtype: String::new(),
            catalog_kind: "WeaponGun".into(),
            tiers: 1,
            tag: String::new(),
            tags: Vec::new(),
            entity: "e-vi".into(),
            raw_product: None,
            recipe: 0,
        };
        let mut book = CraftBook::default();
        book.items.push(item);
        let mut values = GameValues {
            recipes: vec![Recipe { product: Some("e-vi".into()), ..Recipe::default() }],
            ..GameValues::default()
        };
        values.items.insert("e-vi".into(), ItemValues { size: Some(2), ..ItemValues::default() });
        let found = kind_and_size(&values, &book, &note("Omnisky VI Cannon"));
        assert_eq!(found, Some((versekit_core::blueprints::kind_readable("WeaponGun"), Some(2))));
        // By the identifier as well, whatever the name on the note.
        let by_reference = Note { reference: "E-VI".into(), ..note("anders geschrieben") };
        assert_eq!(kind_and_size(&values, &book, &by_reference).map(|(_, size)| size), Some(Some(2)));
        assert_eq!(kind_and_size(&values, &book, &note("Unbekanntes Teil")), None);
    }

    /// A data folder of its own with one ship in the stored erkul file.
    fn folder(name: &str) -> Paths {
        use versekit_core::paths::Host;
        use versekit_core::pypath::Flavor;
        let dir = std::env::temp_dir().join(format!("versekit-farm-{}-{name}", versekit_core::testdir::run_tag()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("folder");
        let text = dir.to_string_lossy().into_owned();
        let paths = Paths::new(Host::new(Flavor::native(), &[(versekit_core::legacy::ENV_HOME, &text)], &text));
        std::fs::write(
            paths.app_file(versekit_core::ships::ERKUL_FILE),
            r#"{"format": 4, "spielversion": "4.0", "hersteller": {}, "schiffe": {"testship": {"id": "test_ship", "name": "Test Ship",
                "plaetze": [], "slots": [
                {"pfad": "c1", "art": "Cooler", "groesse": 2, "werk": {"ref": "f-cooler", "name": "Factory Cooler"}},
                {"pfad": "c2", "art": "Cooler", "groesse": 2, "werk": {"ref": "f-cooler", "name": "Factory Cooler"}},
                {"pfad": "s1", "art": "PowerPlant", "groesse": 1}]}}}"#,
        )
        .expect("erkul file");
        paths
    }

    #[test]
    fn a_part_on_build_of_a_ship_is_farmed_without_any_note() {
        let _language = crate::language_lock();
        set_language(Choice::De);
        let paths = folder("hangar");
        // Grand Core takes Taranite; the cooler is bought, the shield is ticked off: only the core counts.
        std::fs::write(
            paths.app_file(NOTES_FILE),
            r#"{"format": 1, "schiffe": [{"name": "Test Ship", "belegung": {
                "s1": {"ref": "c", "name": "Grand Core", "weg": "bauen"},
                "c1": {"ref": "a", "name": "Frostline", "weg": "kaufen"},
                "c2": {"ref": "b", "name": "Arc Shield", "weg": "bauen", "erledigt": true}}}]}"#,
        )
        .expect("fleet file");
        let mut state = FarmState::with_files(&paths);
        let mut crafting = CraftingState::default();
        let (shown, over) = draw(&context(), &mut state, &mut crafting, 900.0);
        assert_eq!(over, 0.0);
        assert!(shown.contains(&t("s_p9_kopf_eins")), "one part of the ship is planned: {shown:?}");
        assert!(shown.iter().any(|s| s == "Taranite"), "the material of the part: {shown:?}");
        assert!(!shown.iter().any(|s| s == "Iron" || s == "Agricium"), "bought and ticked off stay out: {shown:?}");
        // No note: the notes say where their button is, the page is not the empty one.
        assert!(shown.contains(&t("s_mz_leer")) && !shown.contains(&t("s_fl_nichts_geplant")), "{shown:?}");
        let _ = std::fs::remove_dir_all(paths.app_folder());
    }

    #[test]
    fn mining_data_fetched_after_the_start_bring_the_places() {
        let _language = crate::language_lock();
        set_language(Choice::De);
        let paths = folder("bergbau");
        std::fs::write(
            paths.app_file(NOTES_FILE),
            r#"{"format": 1, "schiffe": [], "merkzettel": [{"name": "Frostline", "anzahl": 2, "weg": "bauen"}]}"#,
        )
        .expect("fleet file");
        let mut state = FarmState::with_files(&paths);
        let mut crafting = CraftingState::default();
        let ctx = context();
        let (shown, _) = draw(&ctx, &mut state, &mut crafting, 900.0);
        assert!(shown.contains(&tf("s_fl_kopf", &[("n", Arg::Int(2))])), "the note is planned: {shown:?}");
        assert!(!shown.contains(&t("s_fl_sammel_kopf")), "no mining data, no places: {shown:?}");
        std::fs::write(paths.app_file(mining_data::FILE), MINING).expect("mining file");
        state.looked_at = f64::NEG_INFINITY;
        let (shown, _) = draw(&ctx, &mut state, &mut crafting, 900.0);
        assert!(shown.contains(&t("s_fl_sammel_kopf")), "the places come with the file: {shown:?}");
        assert!(shown.iter().any(|s| s.contains("Daymar (Stanton)")), "{shown:?}");
        let _ = std::fs::remove_dir_all(paths.app_folder());
    }

    #[test]
    fn the_page_fits_at_every_size_and_width_in_both_languages() {
        let _language = crate::language_lock();
        let mut findings = Vec::new();
        for lang in [Choice::De, Choice::En] {
            set_language(lang);
            for points in versekit_core::font_size::POINTS_RANGE.0..=versekit_core::font_size::POINTS_RANGE.1 {
                for width in [380.0, 520.0, 900.0] {
                    let ctx = context();
                    crate::apply_size(&ctx, points);
                    let mut crafting = CraftingState::default();
                    let struck = Note {
                        name: "A very long name of a part that was struck".repeat(2),
                        reference: String::new(),
                        count: 12,
                        way: "bauen".into(),
                        done: false,
                    };
                    let mut state = FarmState { mining: mining(), struck: Some((struck, 0.0)), ..FarmState::default() };
                    let (_, over) =
                        (0..3).map(|_| draw(&ctx, &mut state, &mut crafting, width)).last().expect("frames");
                    if over > 0.5 {
                        findings.push(format!("{lang:?} Größe {points} Breite {width}: {over:.0} zu breit"));
                    }
                }
            }
        }
        set_language(Choice::De);
        assert!(findings.is_empty(), "{}", findings.join("\n"));
    }
}
