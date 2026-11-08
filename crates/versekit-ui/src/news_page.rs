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

//! The page „Was ist neu“ on the user's own data (4.12 `pages._whats_new`): the versions from the change log
//! built into the program and from the release texts of GitHub, a filter by kind, only the newest version open.
//! The head of a version names the version only, no date: updates come almost daily.

use std::collections::HashSet;

use egui::{RichText, Sense, Ui};
use versekit_core::changelog::{self, Entry, Kind, MOST_ENTRIES};
use versekit_core::language::{Arg, Lang, current, t, tf};
use versekit_core::theme::{IconTint, Scheme};
use versekit_core::updater::Release;

use crate::color;
use crate::layout::GAP;
use crate::widgets::{self, ButtonKind};

/// One version of the page with what is read from its text once.
struct Shown {
    entry: Entry,
    lead: String,
    points: Vec<(Kind, String)>,
}

/// What the page keeps: the filter, which versions were opened or closed by hand, and the history it built.
#[derive(Default)]
pub struct NewsState {
    /// `None`: everything.
    filter: Option<Kind>,
    /// The versions (by place in the list) whose state is the opposite of the one the filter gives them.
    flipped: HashSet<usize>,
    built: Option<(Lang, Vec<String>, Vec<Shown>)>,
    /// Made-up versions stand in (checks only): the history is not built again.
    #[cfg(test)]
    fixed: bool,
}

/// The kinds with their filter button, everything first.
fn filters() -> [(Option<Kind>, String); 4] {
    [
        (None, t("s_wn_f_alle")),
        (Some(Kind::New), t(Kind::New.key())),
        (Some(Kind::Better), t(Kind::Better.key())),
        (Some(Kind::Fix), t(Kind::Fix.key())),
    ]
}

/// The versions the page lists: at most [`MOST_ENTRIES`], each with its lead and its points read once.
fn shown_of(entries: Vec<Entry>) -> Vec<Shown> {
    entries
        .into_iter()
        .take(MOST_ENTRIES)
        .map(|entry| Shown {
            lead: changelog::intro(&entry.text),
            points: changelog::points_by_kind(&entry.text),
            entry,
        })
        .collect()
}

impl NewsState {
    /// The versions of the history in the language in force, newest first, at most [`MOST_ENTRIES`]. Built
    /// again only when the language or the list of releases changed.
    fn build(&mut self, releases: &[Release]) {
        #[cfg(test)]
        if self.fixed {
            return;
        }
        let lang = current();
        let stale = self
            .built
            .as_ref()
            .is_none_or(|(l, names, _)| *l != lang || !names.iter().eq(releases.iter().map(|r| &r.version)));
        if stale {
            let names: Vec<String> = releases.iter().map(|r| r.version.clone()).collect();
            let shown = shown_of(changelog::listed(lang, releases));
            self.flipped.clear();
            self.built = Some((lang, names, shown));
        }
    }

    /// The filter chosen now (for the checks).
    pub fn filter(&self) -> Option<Kind> {
        self.filter
    }

    /// How many versions the page lists now (checks only).
    #[cfg(test)]
    pub(crate) fn listed(&self) -> usize {
        self.built.as_ref().map_or(0, |(_, _, shown)| shown.len())
    }

    /// Puts made-up versions in the place of the history (checks only); the page draws them when it is given an
    /// empty list of releases.
    #[cfg(test)]
    pub(crate) fn set_entries(&mut self, entries: Vec<Entry>) {
        let shown = shown_of(entries);
        self.flipped.clear();
        self.fixed = true;
        self.built = Some((current(), Vec::new(), shown));
    }

    /// The page. ⚠ Only the newest version is open, or all of them while a filter is on: whoever reported a bug
    /// looks for exactly that.
    pub fn show(&mut self, ui: &mut Ui, scheme: &Scheme, releases: &[Release]) {
        ui.add(egui::Label::new(RichText::new(t("s_wn_lead")).small().color(color(scheme.sub))).wrap());
        ui.add_space(GAP);
        // The head card: the filters and, when no version has a change of the kind chosen, the line that says so.
        widgets::card(ui, scheme, |ui| {
            widgets::card_title(ui, scheme, &t("s_wn_auswahl"));
            ui.horizontal_wrapped(|row| {
                for (kind, word) in filters() {
                    let button_kind = if self.filter == kind { ButtonKind::Strong } else { ButtonKind::Normal };
                    if row.add(widgets::Button::new(&word, button_kind, scheme)).clicked() && self.filter != kind {
                        self.filter = kind;
                        self.flipped.clear();
                    }
                }
            });
            self.build(releases);
            let filter = self.filter;
            let entries = self.built.as_ref().map_or(&[][..], |(_, _, shown)| shown.as_slice());
            let any = entries
                .iter()
                .any(|shown| shown.points.iter().any(|(kind, _)| filter.is_none_or(|wanted| wanted == *kind)));
            if !any {
                ui.add_space(GAP);
                ui.label(RichText::new(t("s_wn_nichts")).small().color(color(scheme.sub)));
            }
        });
        ui.add_space(GAP);
        let filter = self.filter;
        let width = badge_width(ui);
        let mut flip = None;
        let mut drawn = 0;
        let entries = self.built.as_ref().map_or(&[][..], |(_, _, shown)| shown.as_slice());
        for (place, shown) in entries.iter().enumerate() {
            let points: Vec<&(Kind, String)> =
                shown.points.iter().filter(|(kind, _)| filter.is_none_or(|wanted| wanted == *kind)).collect();
            if points.is_empty() {
                continue;
            }
            drawn += 1;
            // The first version that is listed, not the first of the history: one without a change of the kind is
            // left out and must not leave the page closed.
            let default_open = drawn == 1 || filter.is_some();
            let open = default_open != self.flipped.contains(&place);
            if version_box(ui, scheme, shown, &points, open, width) {
                flip = Some(place);
            }
            ui.add_space(4.0);
        }
        if let Some(place) = flip
            && !self.flipped.remove(&place)
        {
            self.flipped.insert(place);
        }
    }
}

/// The width every badge of the kinds gets: the widest word, so that the texts beside them start in one line.
fn badge_width(ui: &Ui) -> f32 {
    let font = egui::TextStyle::Small.resolve(ui.style());
    let widest = Kind::ALL
        .iter()
        .map(|kind| {
            ui.fonts_mut(|fonts| fonts.layout_no_wrap(t(kind.key()), font.clone(), egui::Color32::WHITE).size().x)
        })
        .fold(0.0_f32, f32::max);
    widest + 12.0
}

fn kind_color(scheme: &Scheme, kind: Kind) -> versekit_core::theme::Color {
    match kind {
        Kind::New => scheme.accent,
        Kind::Better => scheme.blue,
        Kind::Fix => scheme.gold,
    }
}

/// A version with its head (a click on any part of it folds it) and, when open, its lead and its points.
/// Returns true when the head was clicked.
fn version_box(
    ui: &mut Ui,
    scheme: &Scheme,
    shown: &Shown,
    points: &[&(Kind, String)],
    open: bool,
    width: f32,
) -> bool {
    let mut clicked = false;
    widgets::card(ui, scheme, |ui| {
        ui.set_width(ui.available_width());
        let background = ui.painter().add(egui::Shape::Noop);
        let head = ui.scope_builder(egui::UiBuilder::new().sense(Sense::click()), |ui| {
            // A label that can be selected takes the click away from the head.
            ui.style_mut().interaction.selectable_labels = false;
            ui.horizontal_wrapped(|row| {
                if let Some(image) = crate::icons::themed(
                    if open { "chevron-down" } else { "chevron-right" },
                    scheme,
                    IconTint::Normal,
                    16.0,
                ) {
                    row.add(image);
                }
                row.label(RichText::new(&shown.entry.version).strong().color(color(scheme.accent)));
                let count = if points.len() == 1 {
                    t("s_wn_aenderung_eins")
                } else {
                    tf("s_wn_aenderungen", &[("0", Arg::from(points.len() as i64))])
                };
                row.label(RichText::new(count.trim()).small().color(color(scheme.sub)));
            })
        });
        let response = head.response.on_hover_cursor(egui::CursorIcon::PointingHand);
        response
            .widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, open, shown.entry.version.clone()));
        if response.hovered() {
            let fill = egui::Shape::rect_filled(response.rect, crate::control_radius(scheme), color(scheme.hover));
            ui.painter().set(background, fill);
        }
        widgets::focus_ring(ui, &response, scheme, crate::control_radius(scheme));
        clicked = response.clicked();
        if open {
            ui.add_space(6.0);
            if !shown.lead.is_empty() {
                ui.add(egui::Label::new(RichText::new(&shown.lead).small().color(color(scheme.sub))).wrap());
                ui.add_space(6.0);
            }
            for (kind, line) in points {
                ui.horizontal_top(|row| {
                    kind_badge(row, scheme, *kind, width);
                    row.add(egui::Label::new(changelog::clean_row(line)).wrap());
                });
                ui.add_space(2.0);
            }
        }
    });
    clicked
}

/// The mark of a kind, `width` points wide.
fn kind_badge(ui: &mut Ui, scheme: &Scheme, kind: Kind, width: f32) {
    let tone = color(kind_color(scheme, kind));
    let word = t(kind.key());
    let font = egui::TextStyle::Small.resolve(ui.style());
    let galley = ui.painter().layout_no_wrap(word.clone(), font, tone);
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, galley.size().y + 2.0), Sense::hover());
    ui.painter().rect_stroke(
        rect,
        crate::control_radius(scheme),
        egui::Stroke::new(1.0, tone),
        egui::StrokeKind::Inside,
    );
    ui.painter().galley(rect.center() - galley.size() / 2.0, galley, tone);
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, &word));
}
