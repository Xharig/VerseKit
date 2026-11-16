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

//! The page "Org stock": what the members of a connected org share, to look
//! at. It changes nothing of the own stock. The rows come from the hub, which
//! holds them in memory only; the page asks for them when it opens and when
//! the member clicks the button, never on a clock.

use std::collections::BTreeMap;

use egui::{RichText, Ui};
use versekit_core::language::{Arg, t, tf};
use versekit_core::orgstd::data::{OrgLot, Unit};
use versekit_core::orgstd::hub::{ConnectionView, Kind};
use versekit_core::theme::Scheme;

use crate::layout::GAP;
use crate::org_data::{OpenWatch, OrgLink, amount_text, columns_line, status_line};
use crate::widgets;

/// How many rows more a click on "show more" adds.
const PAGE_ROWS: usize = 100;
/// The least widths of the columns: ware, amount, quality, place, member.
const LEAST: [f32; 5] = [120.0, 80.0, 36.0, 90.0, 80.0];
/// The shares of what is left over.
const SHARES: [f32; 5] = [0.35, 0.1, 0.05, 0.3, 0.2];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Column {
    Item,
    Amount,
    Quality,
    Place,
    Member,
}

const COLUMNS: [(Column, &str); 5] = [
    (Column::Item, "s_ol_sp_ware"),
    (Column::Amount, "s_ml_sp_menge"),
    (Column::Quality, "s_ml_sp_q"),
    (Column::Place, "s_ml_sp_ort"),
    (Column::Member, "s_os_mitglied"),
];

/// What the page keeps between frames: the choice of the org, the filters, the sorting; fresh data never resets
/// them.
pub struct OrgLotsState {
    watch: OpenWatch,
    org: String,
    search: String,
    /// 0 all, 1 material (SCU), 2 items (pieces).
    kind: usize,
    place: usize,
    sort: (Column, bool),
    sums: bool,
    shown_rows: usize,
}

impl Default for OrgLotsState {
    fn default() -> Self {
        OrgLotsState {
            watch: OpenWatch::default(),
            org: String::new(),
            search: String::new(),
            kind: 0,
            place: 0,
            sort: (Column::Item, true),
            sums: false,
            shown_rows: PAGE_ROWS,
        }
    }
}

fn lower(text: &str) -> String {
    text.to_lowercase()
}

/// Whether a row passes the search and the filters.
fn passes(state: &OrgLotsState, places: &[String], lot: &OrgLot) -> bool {
    let kind_ok = match state.kind {
        1 => lot.unit != Unit::Piece,
        2 => lot.unit == Unit::Piece,
        _ => true,
    };
    let place_ok = state.place == 0 || places.get(state.place - 1).is_some_and(|p| p == &lot.location);
    let words: Vec<String> = lower(state.search.trim()).split_whitespace().map(str::to_string).collect();
    let hay = lower(&format!("{} {} {}", lot.item, lot.location, lot.member));
    kind_ok && place_ok && words.iter().all(|w| hay.contains(w))
}

/// The rows in the order the page shows them; a row without a value in the sorted column stands last whichever
/// way the column is sorted.
fn arranged(state: &OrgLotsState, lots: &[OrgLot], places: &[String]) -> Vec<usize> {
    let mut rows: Vec<usize> = (0..lots.len()).filter(|n| passes(state, places, &lots[*n])).collect();
    let (column, ascending) = state.sort;
    let value_less = |a: &OrgLot, b: &OrgLot| match column {
        Column::Item => lower(&a.item).cmp(&lower(&b.item)),
        Column::Amount => a.quantity.total_cmp(&b.quantity),
        Column::Quality => a.quality.cmp(&b.quality),
        Column::Place => lower(&a.location).cmp(&lower(&b.location)),
        Column::Member => lower(&a.member).cmp(&lower(&b.member)),
    };
    let unknown = |lot: &OrgLot| match column {
        Column::Quality => lot.quality.is_none(),
        Column::Place => lot.location.is_empty(),
        Column::Member => lot.member.is_empty(),
        _ => false,
    };
    rows.sort_by(|a, b| {
        let (a, b) = (&lots[*a], &lots[*b]);
        match (unknown(a), unknown(b)) {
            (true, false) => std::cmp::Ordering::Greater,
            (false, true) => std::cmp::Ordering::Less,
            _ => {
                let order = value_less(a, b);
                let order = if ascending { order } else { order.reverse() };
                order.then_with(|| lower(&a.item).cmp(&lower(&b.item)))
            }
        }
    });
    rows
}

/// One line of the list: the cells of `lot`.
fn lot_cells(lot: &OrgLot) -> [String; 5] {
    [
        lot.item.clone(),
        amount_text(lot.quantity, lot.unit),
        lot.quality.map(|q| q.to_string()).unwrap_or_default(),
        lot.location.clone(),
        lot.member.clone(),
    ]
}

fn heads(ui: &mut Ui, scheme: &Scheme, state: &mut OrgLotsState) {
    let mut clicked = None;
    columns_line(ui, &LEAST, &SHARES, |cell, at| {
        let (column, key) = COLUMNS[at];
        let sorted = (state.sort.0 == column).then_some(state.sort.1);
        if cell.horizontal_wrapped(|head| widgets::sort_head(head, scheme, &t(key), sorted, false)).inner {
            clicked = Some(column);
        }
    });
    if let Some(column) = clicked {
        state.sort = if state.sort.0 == column { (column, !state.sort.1) } else { (column, true) };
    }
}

fn row(ui: &mut Ui, lot: &OrgLot) {
    let cells = lot_cells(lot);
    columns_line(ui, &LEAST, &SHARES, |cell, at| {
        let text = RichText::new(&cells[at]);
        cell.add(egui::Label::new(if at == 0 { text.strong() } else { text }).wrap());
    });
}

/// The rows summed per ware and unit, each folding open to its rows.
fn sums(ui: &mut Ui, lots: &[OrgLot], rows: &[usize]) {
    let mut groups: BTreeMap<(String, &'static str), Vec<&OrgLot>> = BTreeMap::new();
    for n in rows {
        let lot = &lots[*n];
        groups.entry((lower(&lot.item), lot.unit.code())).or_default().push(lot);
    }
    for ((_, _), members) in groups {
        let first = members[0];
        let total: f64 = members.iter().map(|l| l.quantity).sum();
        let who: std::collections::BTreeSet<&str> = members.iter().map(|l| l.member.as_str()).collect();
        let title = format!(
            "{} · {} · {}",
            first.item,
            amount_text(total, first.unit),
            tf("s_ol_mitglieder", &[("n", Arg::from(who.len() as i64))])
        );
        widgets::collapsing(&title).id_salt(("org-summe", &first.item, first.unit.code())).show(ui, |ui| {
            for lot in members {
                row(ui, lot);
            }
        });
    }
}

/// Draws the page "Org stock".
pub fn show_page(ui: &mut Ui, scheme: &Scheme, state: &mut OrgLotsState, link: &OrgLink) {
    let orgs = link.orgs(Kind::Stock);
    widgets::hint(ui, scheme, &t("s_ol_lead"));
    ui.add_space(6.0);
    let Some(org) = pick_org(ui, state, &orgs) else {
        widgets::empty_state(ui, scheme, "warehouse", &t("s_ol_keine_orga"));
        return;
    };
    let ctx = ui.ctx().clone();
    if state.watch.opened(&ctx) {
        link.open(&org.id, Kind::Stock);
    }
    let data = link.hub.data(&org.id);
    let now = (versekit_core::orgstd::hub::system_clock())();
    let acts = status_line(ui, scheme, &org, data.stock.info(), now, link.sample);
    if acts.refresh && !link.sample {
        link.hub.refresh(&org.id, Kind::Stock);
    }
    if acts.check {
        link.hub.check(&org.id);
    }
    if link.hub.working() {
        ctx.request_repaint_after(std::time::Duration::from_millis(200));
    }
    ui.add_space(GAP);
    let lots = data.stock.rows.clone();
    let mut places: Vec<String> = lots.iter().filter(|l| !l.location.is_empty()).map(|l| l.location.clone()).collect();
    places.sort_by_key(|p| lower(p));
    places.dedup();
    if state.place > places.len() {
        state.place = 0;
    }
    titled_card(ui, scheme, &t("s_af_filter"), |ui| {
        let width = ui.available_width();
        widgets::search_field(ui, &mut state.search, &t("s_sl_suche"), scheme, width);
        ui.add_space(4.0);
        let kinds = [t("s_ml_alle_arten"), t("s_ml_material"), t("s_ml_items")];
        let place_words: Vec<String> = std::iter::once(t("s_ml_alle_orte")).chain(places.iter().cloned()).collect();
        let (kind_refs, place_refs): (Vec<&str>, Vec<&str>) =
            (kinds.iter().map(String::as_str).collect(), place_words.iter().map(String::as_str).collect());
        let mut chosen = [state.kind, state.place];
        if widgets::filter_bar(ui, "org-lager", &[&kind_refs, &place_refs], &mut chosen) {
            (state.kind, state.place) = (chosen[0], chosen[1]);
            state.shown_rows = PAGE_ROWS;
        }
        ui.add_space(4.0);
        widgets::switch_row_help(ui, scheme, &t("s_ol_summe"), &t("s_ol_summe_h"), &mut state.sums);
    });
    ui.add_space(GAP);
    titled_card(ui, scheme, &t("s_ol_bestand"), |ui| {
        let rows = arranged(state, &lots, &places);
        let line = tf("s_ol_zahl", &[("shown", Arg::from(rows.len() as i64)), ("total", Arg::from(lots.len() as i64))]);
        ui.label(RichText::new(line).small().color(crate::color(scheme.sub)));
        ui.add_space(4.0);
        if rows.is_empty() {
            let text = if lots.is_empty() { t("s_ol_leer") } else { t("s_ol_keine_treffer") };
            widgets::empty_state(ui, scheme, "warehouse", &text);
            return;
        }
        if state.sums {
            sums(ui, &lots, &rows);
            return;
        }
        widgets::list_frame(ui, scheme, |ui| {
            ui.set_min_width(ui.available_width());
            heads(ui, scheme, state);
            for n in rows.iter().take(state.shown_rows) {
                ui.separator();
                row(ui, &lots[*n]);
            }
        });
        let left = rows.len().saturating_sub(state.shown_rows);
        if left > 0 {
            ui.add_space(4.0);
            let word = tf("s_af_mehr", &[("n", Arg::from(left.min(PAGE_ROWS) as i64))]);
            if ui.add(widgets::Button::new(&word, widgets::ButtonKind::Normal, scheme)).clicked() {
                state.shown_rows += PAGE_ROWS;
            }
        }
    });
}

/// The org the page shows: the one chosen if it still shows, else the first; with several a choice stands on top.
fn pick_org(ui: &mut Ui, state: &mut OrgLotsState, orgs: &[ConnectionView]) -> Option<ConnectionView> {
    let at = orgs.iter().position(|o| o.id == state.org).unwrap_or(0);
    let org = orgs.get(at)?.clone();
    state.org = org.id.clone();
    if orgs.len() > 1 {
        let names: Vec<String> = orgs.iter().map(|o| if o.tag.is_empty() { o.name.clone() } else { format!("{} [{}]", o.name, o.tag) }).collect();
        let refs: Vec<&str> = names.iter().map(String::as_str).collect();
        let mut chosen = [at];
        if widgets::filter_bar(ui, "org-lager-orga", &[&refs], &mut chosen) {
            state.org = orgs[chosen[0]].id.clone();
            state.shown_rows = PAGE_ROWS;
        }
        ui.add_space(4.0);
        return orgs.iter().find(|o| o.id == state.org).cloned();
    }
    Some(org)
}

/// A card with `title` over `add`.
fn titled_card(ui: &mut Ui, scheme: &Scheme, title: &str, add: impl FnOnce(&mut Ui)) {
    widgets::card(ui, scheme, |ui| {
        ui.set_min_width(ui.available_width());
        widgets::card_title(ui, scheme, title);
        ui.add_space(4.0);
        add(ui);
    });
}

#[cfg(test)]
mod tests;
