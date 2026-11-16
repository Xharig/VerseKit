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

//! The plan on the crafting page: the card "Vorgemerkt" with every part the farm list counts (the farm notes, and the
//! parts on "build" of the ships of the hangar and of the wish list), and how often each item is planned for the
//! list and the item.
//!
//! One calculation serves the card, the counts and the farm list ([`farm::plan_lines`]), so the page of the farm
//! list and this one name the same number. The farm notes can be changed here; the parts of a ship belong to the
//! hangar and are only read.

use std::collections::{HashMap, HashSet};

use egui::{RichText, Ui};
use versekit_core::hangar;
use versekit_core::jsonio::Map;
use versekit_core::language::{Arg, t, tf};
use versekit_core::notepad::{self, Note};
use versekit_core::stock_data::lower;
use versekit_core::theme::Scheme;
use versekit_gamedata::farm::{self, PlanCount, PlanLine, PlanSource};

use super::{Action, CraftingState, NOTES_FILE, sub};
use crate::color;
use crate::farm_page::{number, row_button};
use crate::widgets::{self, ButtonKind};

/// The width of the count between minus and plus, so that the plus stays where it is.
const COUNT_WIDTH: f32 = 70.0;
/// So long a struck note can be taken back.
const UNDO_SECONDS: f64 = 10.0;
/// How high the groups of the open card stand at most; the rest scrolls.
const BODY_HEIGHT: f32 = 240.0;

/// What the page keeps of the plan between frames.
#[derive(Default)]
pub(super) struct Plan {
    lines: Vec<PlanLine>,
    counts: HashMap<usize, PlanCount>,
    /// What the lines were worked out from: the version of the hangar, the notes and the slots, and the number of
    /// items in the book.
    worked_from: Option<(u64, usize)>,
    /// The note struck last and when, to take it back.
    struck: Option<(Note, f64)>,
    /// A write of the notes failed.
    save_failed: bool,
    /// The card is open; kept in the settings.
    pub(super) open: bool,
}

impl Plan {
    /// How many parts the plan holds, as the farm list counts them.
    pub(super) fn pieces(&self) -> i64 {
        self.lines.iter().map(|l| l.count).sum()
    }
}

impl CraftingState {
    /// Works the plan out again when the hangar, the notes, the slots or the recipes changed since.
    pub(super) fn refresh_plan(&mut self) {
        let key = (self.ship_version, self.book.items.len());
        if self.plan.worked_from == Some(key) {
            return;
        }
        self.plan.worked_from = Some(key);
        let Some(values) = &self.values else {
            self.plan.lines.clear();
            self.plan.counts.clear();
            return;
        };
        let slots = |ship: &Map, chosen: &Map| self.erkul.cart_slots(ship, chosen);
        self.plan.lines = farm::plan_lines(&self.notes, values, &self.book, Some(&slots));
        self.plan.counts = farm::plan_counts(&self.book, &self.plan.lines);
    }

    /// How often the item is planned, by the notes and by the ships.
    pub(super) fn plan_count(&self, index: usize) -> PlanCount {
        self.plan.counts.get(&index).copied().unwrap_or_default()
    }

    /// Changes the farm notes and writes them: reads them again first, so that another place's change stays.
    pub(super) fn edit_notes(&mut self, change: impl FnOnce(&mut Map)) {
        let Some(paths) = &self.paths else {
            change(&mut self.notes);
            self.ship_version += 1;
            return;
        };
        let file = paths.app_file(NOTES_FILE);
        let _lock = hangar::lock();
        let mut held = hangar::load_held(&file);
        change(&mut held.data);
        self.plan.save_failed = !hangar::save_held(&file, &mut held).ok();
        self.notes = held.data;
        self.notes_seen = Self::file_time(&file);
        self.ship_version += 1;
    }

    /// The index of the item a part is called, as the list names it, else by the name of its product.
    fn item_named(&self, name: &str) -> Option<usize> {
        self.book
            .items
            .iter()
            .position(|item| item.name == name)
            .or_else(|| self.book.items.iter().position(|item| lower(&item.basis) == lower(name)))
    }
}

/// The row "Geplant" of the chosen item: how often the farm notes plan it, with less and more; a plan of ships is
/// added as text, it belongs to the hangar. A plus without a note puts the item on the notes.
pub(super) fn row(ui: &mut Ui, scheme: &Scheme, state: &mut CraftingState, index: usize) {
    let count = state.plan_count(index);
    let Some(name) = state.book.items.get(index).map(|item| item.name.clone()) else { return };
    let mut change: Option<i64> = None;
    ui.add_space(4.0);
    ui.horizontal_wrapped(|row| {
        row.label(sub(scheme, t("s_fg_pl_geplant")));
        let less = match count.notes {
            0 => t("s_fg_pl_weniger_null"),
            1 => t("s_fg_pl_weniger_aus"),
            _ => t("s_fg_pl_weniger"),
        };
        if row_button(row, scheme, "minus", &less, &name, count.notes > 1) {
            change = Some(count.notes - 1);
        }
        row.add_sized(
            [COUNT_WIDTH, row.spacing().interact_size.y],
            egui::Label::new(format!("{} {}", count.notes, t("s_mz_stueck"))),
        );
        if row_button(row, scheme, "plus", &t("s_fg_pl_mehr"), &name, true) {
            change = Some(count.notes + 1);
        }
        if count.ships > 0 {
            row.label(sub(scheme, tf("s_fg_pl_plus_schiffe", &[("k", Arg::from(count.ships))])));
        }
    });
    if let Some(wanted) = change {
        state.edit_notes(|data| {
            if notepad::contains(data, &name) {
                notepad::set_count(data, &name, wanted);
            } else {
                notepad::add(data, &name, "", wanted.max(1));
            }
        });
    }
}

/// The title of the card: the number of parts the plan holds.
fn title(pieces: i64) -> String {
    match pieces {
        0 => t("s_fg_pl_titel_leer"),
        1 => t("s_fg_pl_titel_eins"),
        n => tf("s_fg_pl_titel", &[("n", Arg::from(n))]),
    }
}

/// One group of the card: where its parts come from.
struct Group<'a> {
    heading: String,
    source: PlanSource,
    lines: Vec<&'a PlanLine>,
}

/// The lines in groups by where they come from: the notes first, then each ship of the hangar, then each ship of the
/// wish list. Within a group by kind, size and name; the same part of a ship once, with its count.
fn groups<'a>(state: &CraftingState, lines: &'a [PlanLine]) -> Vec<Group<'a>> {
    let mut groups: Vec<Group<'a>> = Vec::new();
    for line in lines {
        let heading = match line.source {
            PlanSource::Notepad => t("s_fg_pl_merkzettel"),
            PlanSource::Hangar => tf("s_fg_pl_kopf_hangar", &[("0", Arg::from(line.ship.as_str()))]),
            PlanSource::Wishlist => tf("s_fg_pl_kopf_wunsch", &[("0", Arg::from(line.ship.as_str()))]),
        };
        match groups.iter_mut().find(|g| g.source == line.source && g.heading == heading) {
            Some(group) => group.lines.push(line),
            None => groups.push(Group { heading, source: line.source, lines: vec![line] }),
        }
    }
    let rank = |source: PlanSource| match source {
        PlanSource::Notepad => 0,
        PlanSource::Hangar => 1,
        PlanSource::Wishlist => 2,
    };
    groups.sort_by_key(|g| rank(g.source));
    for group in &mut groups {
        let key = |line: &PlanLine| {
            let (kind, size) = state
                .values
                .as_ref()
                .and_then(|values| crate::farm_page::kind_and_size(values, &state.book, &line.name, &line.reference))
                .unwrap_or_default();
            (kind.is_empty(), lower(&kind), versekit_core::size_order::key(size, &line.name))
        };
        group.lines.sort_by_cached_key(|line| key(line));
    }
    groups
}

/// The same part of one ship once, with the sum of its slots.
fn merged<'a>(lines: &[&'a PlanLine]) -> Vec<(&'a PlanLine, i64)> {
    let mut rows: Vec<(&PlanLine, i64)> = Vec::new();
    for line in lines {
        match rows.iter_mut().find(|(known, _)| lower(&known.name) == lower(&line.name)) {
            Some((_, count)) => *count += line.count,
            None => rows.push((line, line.count)),
        }
    }
    rows
}

/// Draws the card under the fabricator: folded, with the number of parts in its title. What a click asks of the
/// window comes back.
pub(super) fn show(ui: &mut Ui, scheme: &Scheme, state: &mut CraftingState) -> Option<Action> {
    state.refresh_plan();
    let mut action = None;
    let head = title(state.plan.pieces());
    let open = state.plan.open;
    let shown = widgets::collapsing(&head).id_salt("vorgemerkt").open(Some(open)).show(ui, |ui| {
        action = body(ui, scheme, state);
    });
    if shown.header_response.clicked() {
        state.plan.open = !open;
    }
    action
}

fn body(ui: &mut Ui, scheme: &Scheme, state: &mut CraftingState) -> Option<Action> {
    let mut action = None;
    if state.plan.save_failed {
        ui.label(RichText::new(t("s_bp_speichern_fehler")).color(color(scheme.red_text())));
    }
    undo_line(ui, scheme, state);
    if state.plan.lines.is_empty() {
        ui.add(egui::Label::new(sub(scheme, t("s_fg_pl_leer"))).wrap());
        return None;
    }
    let lines = state.plan.lines.clone();
    let list = state.farm_list_of(&lines);
    let short: HashSet<String> =
        list.as_ref().map(|l| l.missing.iter().map(|n| lower(n.material.trim())).collect()).unwrap_or_default();
    let mut set_count: Option<(String, i64)> = None;
    let mut strike: Option<Note> = None;
    // The groups scroll within a height of their own, like the jobs of the fabricator: the lists below keep their room
    // however much is planned. The way to the farm list stands below, always in reach.
    egui::ScrollArea::vertical().id_salt("vorgemerkt-liste").max_height(BODY_HEIGHT).auto_shrink([false, true]).show(
        ui,
        |ui| {
            for group in groups(state, &lines) {
                ui.add_space(6.0);
                ui.label(RichText::new(&group.heading).small().strong().color(color(scheme.sub)));
                match group.source {
                    PlanSource::Notepad => {
                        for (k, line) in group.lines.iter().enumerate() {
                            note_row(ui, scheme, state, line, k, &short, &mut set_count, &mut strike);
                        }
                    }
                    PlanSource::Hangar | PlanSource::Wishlist => {
                        for (k, (line, count)) in merged(&group.lines).into_iter().enumerate() {
                            ship_row(ui, scheme, state, line, count, k, &short, &mut action);
                        }
                    }
                }
            }
        },
    );
    if let Some((name, count)) = set_count {
        state.edit_notes(|data| {
            notepad::set_count(data, &name, count);
        });
    }
    if let Some(note) = strike {
        state.edit_notes(|data| {
            notepad::remove(data, &note.name);
        });
        state.plan.struck = Some((note, ui.input(|i| i.time)));
    }
    ui.add_space(6.0);
    footer(ui, scheme, list.as_ref(), &mut action);
    action
}

/// The note struck last can be taken back for a few seconds.
fn undo_line(ui: &mut Ui, scheme: &Scheme, state: &mut CraftingState) {
    let now = ui.input(|i| i.time);
    if state.plan.struck.as_ref().is_some_and(|(_, at)| now - at > UNDO_SECONDS) {
        state.plan.struck = None;
    }
    let Some((note, _)) = state.plan.struck.clone() else { return };
    ui.ctx().request_repaint_after(std::time::Duration::from_millis(500));
    ui.horizontal_wrapped(|row| {
        row.label(sub(scheme, tf("s_p9_gestrichen", &[("0", Arg::from(note.name.as_str()))])));
        let undo = row.add(egui::Link::new(RichText::new(t("s_p9_rueckgaengig")).small()));
        undo.widget_info(|| {
            egui::WidgetInfo::labeled(
                egui::accesskit::Role::Link,
                true,
                format!("{}: {}", note.name, t("s_p9_rueckgaengig")),
            )
        });
        if undo.clicked() {
            state.edit_notes(|data| {
                notepad::restore(data, &note);
            });
            state.plan.struck = None;
        }
    });
}

/// The row of a note: less, the count, more, the name that chooses the item, the strike at the right edge, below it
/// what the part needs.
#[allow(clippy::too_many_arguments)]
fn note_row(
    ui: &mut Ui,
    scheme: &Scheme,
    state: &mut CraftingState,
    line: &PlanLine,
    k: usize,
    short: &HashSet<String>,
    set_count: &mut Option<(String, i64)>,
    strike: &mut Option<Note>,
) {
    let frame = egui::Frame::new()
        .fill(if k % 2 == 1 { color(scheme.bar) } else { egui::Color32::TRANSPARENT })
        .inner_margin(egui::Margin::symmetric(6, 3));
    frame.show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal_wrapped(|row| {
            let less = t(if line.count > 1 { "s_fg_pl_weniger" } else { "s_fg_pl_weniger_aus" });
            if row_button(row, scheme, "minus", &less, &line.name, line.count > 1) {
                *set_count = Some((line.name.clone(), line.count - 1));
            }
            // A fixed width, so the plus does not wander with the digits.
            row.add_sized(
                [COUNT_WIDTH, row.spacing().interact_size.y],
                egui::Label::new(format!("{} {}", line.count, t("s_mz_stueck"))),
            );
            if row_button(row, scheme, "plus", &t("s_fg_pl_mehr"), &line.name, true) {
                *set_count = Some((line.name.clone(), line.count + 1));
            }
            name_link(row, scheme, state, line);
            row.with_layout(egui::Layout::right_to_left(egui::Align::Center), |right| {
                if row_button(right, scheme, "trash-2", &t("s_mz_weg"), &line.name, true) {
                    *strike = notepad::notes(&state.notes).into_iter().find(|note| note.name == line.name);
                }
            });
        });
        material_line(ui, scheme, state, line, line.count, short);
    });
}

/// The row of a part of a ship: only read, the hangar owns it; the name chooses the item.
#[allow(clippy::too_many_arguments)]
fn ship_row(
    ui: &mut Ui,
    scheme: &Scheme,
    state: &mut CraftingState,
    line: &PlanLine,
    count: i64,
    k: usize,
    short: &HashSet<String>,
    action: &mut Option<Action>,
) {
    let frame = egui::Frame::new()
        .fill(if k % 2 == 1 { color(scheme.bar) } else { egui::Color32::TRANSPARENT })
        .inner_margin(egui::Margin::symmetric(6, 3));
    frame.show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal_wrapped(|row| {
            row.add_sized(
                [COUNT_WIDTH, row.spacing().interact_size.y],
                egui::Label::new(format!("{count} {}", t("s_mz_stueck"))),
            );
            name_link(row, scheme, state, line);
        });
        // The way to the hangar has a line of its own: at the same place in every row, whatever the name is.
        ui.horizontal_wrapped(|row| {
            row.add_space(12.0);
            let hangar = row.add(egui::Link::new(RichText::new(t("s_fg_pl_im_hangar_oeffnen")).small()));
            hangar.widget_info(|| {
                egui::WidgetInfo::labeled(
                    egui::accesskit::Role::Link,
                    true,
                    format!("{}: {}", line.name, t("s_fg_pl_im_hangar_oeffnen")),
                )
            });
            if hangar.on_hover_text(t("s_fg_pl_schiff")).clicked() {
                *action = Some(Action::ToHangar);
            }
        });
        material_line(ui, scheme, state, line, count, short);
    });
}

/// The name of a part as a link that chooses the item in the list.
fn name_link(ui: &mut Ui, scheme: &Scheme, state: &mut CraftingState, line: &PlanLine) {
    // A part the book does not know has nothing to choose: its name is only a text.
    let Some(index) = state.item_named(&line.name) else {
        ui.label(RichText::new(&line.name).color(color(scheme.sub)));
        return;
    };
    let name = RichText::new(&line.name).color(color(scheme.accent));
    if ui.add(egui::Link::new(name)).on_hover_text(t("s_fg_pl_waehlen")).clicked() {
        state.choose(index);
        state.scroll_to = Some(index);
    }
}

/// What one part needs, by its recipe: gold where the stock does not cover it.
fn material_line(
    ui: &mut Ui,
    scheme: &Scheme,
    state: &CraftingState,
    line: &PlanLine,
    count: i64,
    short: &HashSet<String>,
) {
    let Some(values) = &state.values else { return };
    let post = farm::Post { name: line.name.clone(), reference: line.reference.clone(), pieces: count };
    let Some(recipe) = farm::recipe_of(values, &state.book, &post) else {
        ui.label(sub(scheme, t("s_mz_kein_rezept")));
        return;
    };
    ui.horizontal_wrapped(|row| {
        row.add_space(12.0);
        for tier in versekit_gamedata::craft_book::ingredients(recipe) {
            for ingredient in tier {
                let needed = ingredient.amount * count as f64;
                let lacks = short.contains(&lower(ingredient.material.trim()));
                let tone = if lacks { scheme.gold } else { scheme.accent };
                let text = format!("{} {}", ingredient.material, super::g(versekit_core::stock::py_round(needed, 4)));
                row.label(RichText::new(tf("s_mz_braucht", &[("0", Arg::from(text))])).small().color(color(tone)));
            }
        }
    });
}

/// What is still missing for the whole plan, and the way to the farm list.
fn footer(ui: &mut Ui, scheme: &Scheme, list: Option<&farm::FarmList>, action: &mut Option<Action>) {
    ui.horizontal_wrapped(|row| {
        match list {
            Some(list) if !list.missing.is_empty() => {
                let names: Vec<String> =
                    list.missing.iter().map(|n| format!("{} {}", n.material, number(n.missing))).collect();
                row.label(RichText::new(tf("s_fg_pl_fehlt", &[("0", Arg::from(names.join(" · ")))])).color(color(scheme.gold)));
            }
            Some(_) => {
                row.label(RichText::new(t("s_fg_pl_reicht")).color(color(scheme.accent)));
            }
            None => {}
        }
        let word = t("s_fg_pl_zur_farm");
        let button = widgets::Button::new(&word, ButtonKind::Normal, scheme).icon("arrow-right");
        if row.add(button).on_hover_text(t("s_fg_pl_zur_farm_tip")).clicked() {
            *action = Some(Action::ToFarm);
        }
    });
}

impl CraftingState {
    /// The farm list of the lines against the stock; `None` while there are no recipes.
    fn farm_list_of(&self, lines: &[PlanLine]) -> Option<farm::FarmList> {
        let values = self.values.as_ref()?;
        Some(farm::farm_list(values, &self.book, &farm::posts_of_plan(lines), &self.stock, &self.data))
    }
}
