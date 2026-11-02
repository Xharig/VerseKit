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

//! The dismantle calculation of the page "Dismantle & Salvage" (`pages._dismantle` of 4.12): which raw
//! materials the fabricator gives back for a part, and which never come back. The rules (the share, the
//! seconds, the blocked materials) come from the game values, the part from the recipes; both through the
//! crafting page, which holds them.
//!
//! Unlike 4.12 the line of the rules follows the values that are loaded, and "nothing is lost" names the share
//! of the data instead of "half".

use egui::{RichText, Ui};
use versekit_core::language::{Arg, t, tf};
use versekit_core::stock_data::lower;
use versekit_core::theme::Scheme;
use versekit_gamedata::craft_book::{self, CraftBook};
use versekit_gamedata::values::GameValues;

use crate::color;
use crate::crafting_page::CraftingState;
use crate::farm_page::number;
use crate::widgets;

fn sub(scheme: &Scheme, text: impl Into<String>) -> RichText {
    RichText::new(text.into()).small().color(color(scheme.sub))
}

/// An amount with its unit: SCU for raw materials, pieces for items (as the crafting page writes them).
fn amount(value: f64, piece: bool) -> String {
    tf(if piece { "s_p9_menge_stk" } else { "s_p9_menge" }, &[("0", Arg::from(number(value)))])
}

/// Every part once, by name, sorted.
pub fn part_names(book: &CraftBook) -> Vec<String> {
    let mut names: Vec<String> = book.items.iter().map(|i| i.basis.clone()).filter(|n| !n.is_empty()).collect();
    names.sort();
    names.dedup();
    names
}

/// Whether taking the part apart has a result to show: a recipe with tiers that names raw materials.
pub fn has_result(values: &GameValues, book: &CraftBook, name: &str) -> bool {
    book.find(values, name)
        .filter(|r| !r.tiers.is_empty())
        .is_some_and(|r| !craft_book::dismantle(values, r).is_empty())
}

/// The line of the rules of the fabricator: the share, the seconds, how many materials never come back.
pub fn rules(ui: &mut Ui, scheme: &Scheme, values: &GameValues) {
    let (share, seconds, blocked) = craft_book::dismantle_rules(values);
    let percent = (share * 100.0).round() as i64;
    let lost_count = craft_book::blocked_count(&blocked);
    let key = if lost_count == 1 { "s_zl_regel_1" } else { "s_zl_regel" };
    let rule = tf(key, &[("prozent", Arg::from(percent)), ("dauer", Arg::from(seconds)), ("n", Arg::from(lost_count))]);
    ui.add(egui::Label::new(sub(scheme, rule)).wrap());
}

/// The calculation for a part: the rules, then the raw materials with what is inside and what comes back,
/// and the line that sums it up. While the game values are not there, what stands in their place.
pub fn show(ui: &mut Ui, scheme: &Scheme, crafting: &mut CraftingState, chosen: &str) {
    if crafting.values_or_wait(ui, scheme).is_none() {
        return;
    }
    let Some((values, book, _)) = crafting.parts() else { return };
    rules(ui, scheme, values);
    ui.add_space(8.0);
    result(ui, scheme, values, book, chosen);
}

fn result(ui: &mut Ui, scheme: &Scheme, values: &GameValues, book: &CraftBook, chosen: &str) {
    let (share, seconds, _) = craft_book::dismantle_rules(values);
    let percent = (share * 100.0).round() as i64;
    let rows = book
        .find(values, chosen)
        .filter(|r| !r.tiers.is_empty())
        .map(|recipe| craft_book::dismantle(values, recipe))
        .unwrap_or_default();
    if rows.is_empty() {
        ui.add(
            egui::Label::new(
                RichText::new(tf("s_zl_kein_rezept", &[("name", Arg::from(chosen))])).color(color(scheme.gold)),
            )
            .wrap(),
        );
        return;
    }
    ui.label(widgets::bold(ui, chosen.to_string(), egui::TextStyle::Body));
    ui.label(sub(scheme, tf("s_zl_dauer", &[("dauer", Arg::from(seconds))])));
    ui.add_space(4.0);
    widgets::list_frame(ui, scheme, |list| {
        list.set_width(list.available_width());
        for (k, row) in rows.iter().enumerate() {
            let frame = egui::Frame::new()
                .fill(if k % 2 == 1 { color(scheme.bar) } else { egui::Color32::TRANSPARENT })
                .inner_margin(egui::Margin::symmetric(6, 3));
            frame.show(list, |line| {
                line.set_width(line.available_width());
                line.horizontal_wrapped(|cells| {
                    let tone = if row.lost { scheme.sub } else { scheme.fg };
                    cells.label(RichText::new(&row.material).color(color(tone)));
                    let inside = amount(row.inside, row.piece);
                    cells.label(sub(scheme, tf("s_zl_drin", &[("menge", Arg::from(inside))])));
                    cells.with_layout(egui::Layout::right_to_left(egui::Align::Center), |right| {
                        if row.lost {
                            right.label(RichText::new(t("s_zl_nichts")).color(color(scheme.yellow)));
                        } else {
                            let back = tf("s_zl_zurueck", &[("menge", Arg::from(amount(row.back, row.piece)))]);
                            right.label(RichText::new(back).color(color(scheme.blue)));
                        }
                    });
                });
            });
        }
    });
    ui.add_space(4.0);
    let lost: Vec<&str> = rows.iter().filter(|r| r.lost).map(|r| r.material.as_str()).collect();
    let said = match lost.as_slice() {
        [] => RichText::new(tf("s_zl_alles_zurueck", &[("prozent", Arg::from(percent))])).color(color(scheme.blue)),
        [one] => RichText::new(tf("s_zl_verloren_1", &[("stoffe", Arg::from(*one))])).color(color(scheme.yellow)),
        many => {
            RichText::new(tf("s_zl_verloren", &[("n", Arg::from(many.len())), ("stoffe", Arg::from(many.join(", ")))]))
                .color(color(scheme.yellow))
        }
    };
    ui.add(egui::Label::new(said).wrap());
}

/// The name as the part list writes it, for a text that names a part: its own name, or a match without case.
pub fn exact_name<'a>(names: &'a [String], typed: &str) -> Option<&'a String> {
    let wanted = lower(typed.trim());
    names.iter().find(|n| lower(n) == wanted)
}
