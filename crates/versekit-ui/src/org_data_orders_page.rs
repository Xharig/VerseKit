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

//! The page "Org orders": the orders of a connected org with their progress,
//! each folding open to its goods, place, due date and note, and, with the
//! right to book, a form in the list to book a delivery.
//!
//! The rows come from the hub; the page asks for them when it opens and when
//! the member clicks the button. A booking is sent once, with a key made when
//! the form opened: sending it again after an error carries the same key, so
//! the org books it once. VerseKit takes nothing out of the own stock for it.

use egui::{RichText, Ui};
use versekit_core::language::{Arg, t, tf};
use versekit_core::orgstd::data::{OrderStatus, OrgOrder};
use versekit_core::orgstd::hub::{ConnectionView, Kind};
use versekit_core::orgstd::{E_NETWORK, E_RATE, E_SCOPE, OrgError};
use versekit_core::theme::Scheme;

use crate::layout::GAP;
use crate::org_data::{OpenWatch, OrgLink, amount_text, columns_line, columns_line_in, status_line};
use crate::widgets::{self, ButtonKind};

/// The least widths of the columns of the list: order, state, progress, due date.
const LEAST: [f32; 4] = [140.0, 70.0, 120.0, 80.0];
/// The shares of what is left over.
const SHARES: [f32; 4] = [0.4, 0.1, 0.3, 0.2];

/// The form of one booking, in the open order.
struct Form {
    order: String,
    /// Made when the form opened, kept until the booking went through or the form closed.
    key: String,
    /// What the member typed per good of the order.
    amounts: Vec<String>,
    /// A request of this form runs or ran and its answer is not taken over yet.
    sent: bool,
    error: Option<String>,
    /// What was asked to book, for the line after it.
    asked: Vec<(String, f64)>,
}

/// What the page keeps between frames.
#[derive(Default)]
pub struct OrgOrdersState {
    watch: OpenWatch,
    org: String,
    /// Also the orders that are done or cancelled.
    closed: bool,
    form: Option<Form>,
    /// The orders whose row is open, by id.
    open: std::collections::BTreeSet<String>,
    /// What the last booking that went through was, to say it under its order.
    booked: Option<(String, String)>,
}

/// How far the goods of an order are delivered, from nothing to one.
pub fn progress_of(order: &OrgOrder) -> f32 {
    let wanted: f64 = order.lines.iter().map(|l| l.quantity).sum();
    if wanted <= 0.0 {
        return 0.0;
    }
    let done: f64 = order.lines.iter().map(|l| l.delivered.min(l.quantity)).sum();
    (done / wanted).clamp(0.0, 1.0) as f32
}

fn status_text(status: OrderStatus) -> String {
    t(match status {
        OrderStatus::Open => "s_oa_offen",
        OrderStatus::Done => "s_oa_erledigt",
        OrderStatus::Cancelled => "s_oa_abgebrochen",
    })
}

/// What a refused booking says, by the status of the answer; nothing of what the server wrote.
pub fn booking_error_text(error: OrgError) -> String {
    if error.code == E_RATE {
        return tf("s_od_f_zu_oft", &[("s", Arg::from(error.retry_after.unwrap_or(60) as i64))]);
    }
    if error.code == E_SCOPE {
        return t("s_od_f_recht");
    }
    if error.code == E_NETWORK || error.status == 0 {
        return t("s_oa_f_netz");
    }
    t(match error.status {
        400 => "s_oa_f_400",
        404 => "s_oa_f_404",
        409 => "s_oa_f_409",
        422 => "s_oa_f_422",
        _ => "s_oa_f_sonst",
    })
}

/// The part of the page that stands for one open order: its facts, its goods and the form.
struct Ask<'a> {
    link: &'a OrgLink,
    org: &'a ConnectionView,
}

fn order_body(ui: &mut Ui, scheme: &Scheme, state: &mut OrgOrdersState, ask: &Ask<'_>, order: &OrgOrder, now: f64) {
    let sub = crate::color(scheme.sub);
    for (name, value) in [
        (t("s_ml_sp_ort"), order.location.clone()),
        (t("s_oa_faellig"), order.due.clone()),
        (t("s_oa_hinweis"), order.note.clone()),
    ] {
        if !value.is_empty() {
            ui.horizontal_wrapped(|row| {
                row.label(RichText::new(format!("{name}:")).color(sub));
                widgets::reading(row, value);
            });
        }
    }
    ui.add_space(4.0);
    for line in &order.lines {
        ui.horizontal_wrapped(|row| {
            row.label(RichText::new(&line.item).strong());
            let text = tf(
                "s_oa_zeile",
                &[
                    ("geliefert", Arg::from(amount_text(line.delivered, line.unit))),
                    ("gesamt", Arg::from(amount_text(line.quantity, line.unit))),
                    ("offen", Arg::from(amount_text(line.open(), line.unit))),
                ],
            );
            row.label(RichText::new(text).color(sub));
        });
    }
    if let Some((id, text)) = &state.booked
        && id == &order.id
    {
        ui.add_space(4.0);
        ui.label(RichText::new(text).color(crate::color(scheme.accent)));
    }
    let may_book = order.status == OrderStatus::Open && ask.org.scopes.iter().any(|s| s == "orders:write");
    if !may_book {
        return;
    }
    ui.add_space(4.0);
    if state.form.as_ref().is_some_and(|f| f.order == order.id) {
        form(ui, scheme, state, ask, order, now);
        return;
    }
    let word = t("s_oa_buchen");
    let spoken = tf("s_oa_buchen_name", &[("titel", Arg::from(order.title.as_str()))]);
    let button = widgets::Button::new(&word, ButtonKind::Normal, scheme).icon("package-plus").spoken(&spoken);
    if ui.add(button).clicked() {
        state.form = Some(Form {
            order: order.id.clone(),
            key: versekit_core::orgstd::share::idempotency_key(),
            amounts: vec![String::new(); order.lines.len()],
            sent: false,
            error: None,
            asked: Vec::new(),
        });
        state.booked = None;
    }
}

/// The form in the open order: a field per good with what is still missing, "Book" and "Cancel".
fn form(ui: &mut Ui, scheme: &Scheme, state: &mut OrgOrdersState, ask: &Ask<'_>, order: &OrgOrder, now: f64) {
    let booking = ask.link.hub.booking(&ask.org.id);
    let running = booking.running && booking.order == order.id;
    let Some(form) = state.form.as_mut() else { return };
    let mut book = false;
    let mut cancel = false;
    widgets::card(ui, scheme, |ui| {
        ui.set_width(ui.available_width());
        widgets::card_title(ui, scheme, &t("s_oa_buchen_titel"));
        ui.add_space(4.0);
        for (line, text) in order.lines.iter().zip(form.amounts.iter_mut()) {
            ui.horizontal_wrapped(|row| {
                row.label(RichText::new(&line.item).strong());
                let open = tf("s_oa_offen_menge", &[("menge", Arg::from(amount_text(line.open(), line.unit)))]);
                row.label(RichText::new(open).color(crate::color(scheme.sub)));
                let name = tf("s_oa_menge_name", &[("ware", Arg::from(line.item.as_str()))]);
                let width = widgets::one_line_width(row, t("s_oa_menge"), egui::TextStyle::Body) + 40.0;
                let response = widgets::checked_field(row, scheme, text, &t("s_oa_menge"), width, None);
                response.widget_info(|| {
                    let mut info = egui::WidgetInfo::text_edit(true, "", text.as_str(), &name);
                    info.label = Some(name.clone());
                    info
                });
            });
        }
        if let Some(error) = &form.error {
            ui.add_space(4.0);
            ui.label(RichText::new(error).color(crate::color(scheme.yellow)));
        }
        ui.add_space(6.0);
        let busy = t("s_os_laeuft");
        let reason = running.then_some(busy.as_str());
        ui.horizontal_wrapped(|row| {
            if row.add(widgets::Button::new(&t("s_oa_buchen_ja"), ButtonKind::Strong, scheme).blocked(reason)).clicked()
            {
                book = true;
            }
            if row.add(widgets::Button::new(&t("s_os_abbrechen"), ButtonKind::Normal, scheme).blocked(reason)).clicked()
            {
                cancel = true;
            }
        });
    });
    if cancel {
        state.form = None;
        return;
    }
    if book {
        send(state, ask, order, now);
    }
    take_answer(state, ask, order);
}

/// Reads the amounts of the form and sends them once.
fn send(state: &mut OrgOrdersState, ask: &Ask<'_>, order: &OrgOrder, now: f64) {
    let Some(form) = state.form.as_mut() else { return };
    let mut items = Vec::new();
    for (line, text) in order.lines.iter().zip(&form.amounts) {
        if text.trim().is_empty() {
            continue;
        }
        match versekit_core::stock::parse_number(text) {
            Some(amount) if amount > 0.0 && amount.is_finite() => items.push((line.item.clone(), amount)),
            _ => {
                form.error = Some(tf("s_oa_keine_zahl", &[("ware", Arg::from(line.item.as_str()))]));
                return;
            }
        }
    }
    if items.is_empty() {
        form.error = Some(t("s_oa_nichts"));
        return;
    }
    form.error = None;
    if ask.link.sample {
        form.error = Some(t("s_oa_vorschau"));
        let _ = now;
        return;
    }
    form.asked = items.clone();
    if ask.link.hub.deliver(&ask.org.id, &order.id, items, &form.key) {
        form.sent = true;
    } else if let Some(Err(error)) = ask.link.hub.booking(&ask.org.id).result {
        form.error = Some(booking_error_text(error));
    }
}

/// Takes the answer of the booking of this form: closes the form on success, says what went wrong otherwise and
/// keeps the key, so that a repeat is the same booking.
fn take_answer(state: &mut OrgOrdersState, ask: &Ask<'_>, order: &OrgOrder) {
    let Some(form) = state.form.as_mut() else { return };
    if !form.sent {
        return;
    }
    let booking = ask.link.hub.booking(&ask.org.id);
    if booking.running || booking.order != order.id {
        return;
    }
    form.sent = false;
    match booking.result {
        Some(Ok(_)) => {
            let listed: Vec<String> = form
                .asked
                .iter()
                .map(|(item, amount)| {
                    let unit = order.lines.iter().find(|l| &l.item == item).map(|l| l.unit);
                    format!("{} {item}", unit.map_or_else(|| amount.to_string(), |u| amount_text(*amount, u)))
                })
                .collect();
            state.booked = Some((order.id.clone(), tf("s_oa_gebucht", &[("liste", Arg::from(listed.join(", ")))])));
            state.form = None;
        }
        Some(Err(error)) => form.error = Some(booking_error_text(error)),
        None => {}
    }
}

/// The row of one order: its title with the arrow, its state, its progress and its due date. Whether it was clicked.
fn order_row(ui: &mut Ui, scheme: &Scheme, order: &OrgOrder, open: bool) -> bool {
    // The whole line is one click area with a fill under the pointer; the fill is painted below the texts.
    let ground = ui.painter().add(egui::Shape::Noop);
    let (id, rect) = columns_line_in(ui, &LEAST, &SHARES, |cell, at| match at {
        0 => {
            cell.horizontal_wrapped(|line| {
                let arrow = if open { "chevron-down" } else { "chevron-right" };
                if let Some(image) = crate::icons::themed(arrow, scheme, versekit_core::theme::IconTint::Normal, 12.0) {
                    line.add(image);
                }
                line.add(egui::Label::new(RichText::new(&order.title).strong()).wrap());
            });
        }
        1 => {
            cell.add(egui::Label::new(status_text(order.status)).wrap());
        }
        2 => {
            let width = cell.available_width();
            widgets::progress(cell, scheme, progress_of(order), width, &order.title);
        }
        _ => {
            cell.add(egui::Label::new(order.due.as_str()).wrap());
        }
    });
    let response = ui.interact(rect, id, egui::Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
    if response.hovered() {
        ui.painter().set(ground, egui::Shape::rect_filled(rect, 3.0, crate::color(scheme.bar)));
    }
    let name = tf("s_oa_zeile_name", &[("titel", Arg::from(order.title.as_str()))]);
    response.widget_info(|| egui::WidgetInfo::selected(egui::accesskit::Role::Button, true, open, &name));
    response.clicked()
}

/// Draws the page "Org orders".
pub fn show_page(ui: &mut Ui, scheme: &Scheme, state: &mut OrgOrdersState, link: &OrgLink) {
    let orgs = link.orgs(Kind::Orders);
    widgets::hint(ui, scheme, &t("s_oa_lead"));
    ui.add_space(6.0);
    let Some(org) = pick_org(ui, state, &orgs) else {
        widgets::empty_state(ui, scheme, "container", &t("s_ol_keine_orga"));
        return;
    };
    let ctx = ui.ctx().clone();
    if state.watch.opened(&ctx) {
        link.open(&org.id, Kind::Orders);
    }
    let data = link.hub.data(&org.id);
    let now_seconds = (versekit_core::orgstd::hub::system_clock())();
    let now = ui.input(|i| i.time);
    let acts = status_line(ui, scheme, &org, data.orders.info(), now_seconds, link.sample);
    if acts.refresh && !link.sample {
        link.hub.refresh(&org.id, Kind::Orders);
    }
    if acts.check {
        link.hub.check(&org.id);
    }
    if link.hub.working() {
        ctx.request_repaint_after(std::time::Duration::from_millis(200));
    }
    ui.add_space(GAP);
    let orders = data.orders.rows.clone();
    let ask = Ask { link, org: &org };
    widgets::card(ui, scheme, |ui| {
        ui.set_min_width(ui.available_width());
        widgets::card_title(ui, scheme, &t("s_oa_titel"));
        ui.add_space(4.0);
        widgets::switch_row(ui, scheme, &t("s_oa_auch_geschlossen"), &mut state.closed);
        ui.add_space(4.0);
        let shown: Vec<&OrgOrder> = orders.iter().filter(|o| state.closed || o.status == OrderStatus::Open).collect();
        if shown.is_empty() {
            let text = if orders.is_empty() { t("s_oa_leer") } else { t("s_oa_keine_offenen") };
            widgets::empty_state(ui, scheme, "container", &text);
            return;
        }
        widgets::list_frame(ui, scheme, |ui| {
            ui.set_min_width(ui.available_width());
            columns_line(ui, &LEAST, &SHARES, |cell, at| {
                let key = ["s_oa_sp_auftrag", "s_oa_sp_status", "s_oa_sp_fortschritt", "s_oa_faellig"][at];
                cell.add(egui::Label::new(RichText::new(t(key)).strong()).wrap());
            });
            for order in shown {
                ui.separator();
                let open = state.open.contains(&order.id);
                if order_row(ui, scheme, order, open) {
                    if open {
                        state.open.remove(&order.id);
                    } else {
                        state.open.insert(order.id.clone());
                    }
                }
                if open {
                    ui.indent(("org-auftrag", order.id.as_str()), |ui| {
                        order_body(ui, scheme, state, &ask, order, now);
                    });
                }
            }
        });
    });
}

/// The org the page shows: the one chosen if it still shows, else the first; with several a choice stands on top.
fn pick_org(ui: &mut Ui, state: &mut OrgOrdersState, orgs: &[ConnectionView]) -> Option<ConnectionView> {
    let at = orgs.iter().position(|o| o.id == state.org).unwrap_or(0);
    let org = orgs.get(at)?.clone();
    state.org = org.id.clone();
    if orgs.len() > 1 {
        let names: Vec<String> = orgs.iter().map(|o| if o.tag.is_empty() { o.name.clone() } else { format!("{} [{}]", o.name, o.tag) }).collect();
        let refs: Vec<&str> = names.iter().map(String::as_str).collect();
        let mut chosen = [at];
        if widgets::filter_bar(ui, "org-auftraege-orga", &[&refs], &mut chosen) {
            state.org = orgs[chosen[0]].id.clone();
            state.form = None;
        }
        ui.add_space(4.0);
        return orgs.iter().find(|o| o.id == state.org).cloned();
    }
    Some(org)
}

#[cfg(test)]
mod tests;
