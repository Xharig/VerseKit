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

//! Sharing the own stock and the own blueprints with an org, on the page of
//! each: a line with the last sharing and the button, and a card in the page
//! that says exactly what would go out before anything does.
//!
//! The standard replaces the whole list, so nothing here sends by itself: a
//! click builds the list and opens the card, "Share" sends once, "Cancel"
//! closes it, and a list that is empty or too long cannot be sent from it.
//! Ending the sharing, the only way an empty list goes out, has a card of its
//! own with its own confirmation.

use egui::{RichText, Ui};
use versekit_core::language::{Arg, t, tf};
use versekit_core::orgstd::hub::{ConnectionView, Thing};
use versekit_core::orgstd::share::{BLUEPRINTS_MAX, BlueprintShare, STOCK_MAX, StockShare, stock_share};
use versekit_core::stock::Row;
use versekit_core::stock_data::StockData;
use versekit_core::theme::Scheme;

use crate::layout::GAP;
use crate::org_data::{OrgLink, amount_text, problem_text};
use crate::widgets::{self, ButtonKind};

/// The most lines of the list the card shows.
const PREVIEW_ROWS: usize = 200;

/// What the card of the page holds.
enum Card {
    Stock(ConnectionView, StockShare),
    Blueprints(ConnectionView, BlueprintShare),
    End(ConnectionView, Thing),
}

/// What the page keeps between frames: the card that is open.
#[derive(Default)]
pub struct OrgShareState {
    card: Option<Card>,
    notice: Option<(String, f64)>,
}

/// What a page offers to share.
pub enum Source<'a> {
    /// The rows of the own stock and the lists of the names that decide which of them are known.
    Stock(&'a [Row], &'a StockData),
    /// Builds the list of the own blueprints when the member asks for it: the file is read then, not in a frame.
    Blueprints(&'a dyn Fn() -> BlueprintShare),
}

impl Source<'_> {
    fn thing(&self) -> Thing {
        match self {
            Source::Stock(..) => Thing::Stock,
            Source::Blueprints(_) => Thing::Blueprints,
        }
    }
}

fn when(seconds: u64) -> String {
    versekit_core::formats::date_time(versekit_core::language::current(), seconds as f64)
}

fn scope_of(thing: Thing) -> &'static str {
    match thing {
        Thing::Stock => "stock:write",
        Thing::Blueprints => "blueprints:write",
    }
}

/// Draws the line of each org the member may share with, and the card when one is open.
pub fn show(ui: &mut Ui, scheme: &Scheme, state: &mut OrgShareState, link: &OrgLink, source: &Source<'_>) {
    let thing = source.thing();
    let orgs = link.writable(scope_of(thing));
    if orgs.is_empty() {
        state.card = None;
        return;
    }
    let ctx = ui.ctx().clone();
    let now = ui.input(|i| i.time);
    let mut opened = None;
    for org in &orgs {
        let shared = link.hub.shared(&org.id, thing);
        ui.horizontal_wrapped(|row| {
            let line = match (shared.at, shared.ended) {
                (Some(at), false) => tf(
                    "s_ot_geteilt",
                    &[
                        ("orga", Arg::from(org.name.as_str())),
                        ("zeit", Arg::from(when(at))),
                        ("n", Arg::from(shared.sent as i64)),
                    ],
                ),
                (Some(at), true) => {
                    tf("s_ot_beendet", &[("orga", Arg::from(org.name.as_str())), ("zeit", Arg::from(when(at)))])
                }
                (None, _) => tf("s_ot_noch_nicht", &[("orga", Arg::from(org.name.as_str()))]),
            };
            row.label(RichText::new(line).small().color(crate::color(scheme.sub)));
            let (word, spoken) = match thing {
                Thing::Stock => (t("s_ot_teilen"), tf("s_ot_teilen_name", &[("orga", Arg::from(org.name.as_str()))])),
                Thing::Blueprints => {
                    (t("s_ot_teilen_bp"), tf("s_ot_teilen_bp_name", &[("orga", Arg::from(org.name.as_str()))]))
                }
            };
            let running = t("s_os_laeuft");
            let reason = shared.running.then_some(running.as_str());
            let share = widgets::Button::new(&word, ButtonKind::Strong, scheme).icon("share-2").spoken(&spoken);
            if row.add(share.blocked(reason)).clicked() {
                opened = Some((org.clone(), false));
            }
            if shared.at.is_some() && !shared.ended {
                let (word, spoken) =
                    (t("s_ot_beenden"), tf("s_ot_beenden_name", &[("orga", Arg::from(org.name.as_str()))]));
                let end = widgets::Button::new(&word, ButtonKind::Normal, scheme).spoken(&spoken);
                if row.add(end.blocked(reason)).clicked() {
                    opened = Some((org.clone(), true));
                }
            }
        });
        if shared.running {
            widgets::loading(ui, scheme, &t("s_os_laeuft"));
            ctx.request_repaint_after(std::time::Duration::from_millis(200));
        }
        if let Some(error) = shared.error {
            let (text, _) = problem_text(error, 0);
            ui.label(RichText::new(text).small().color(crate::color(scheme.yellow)));
        }
        if let (Some(stored), true) = (shared.stored, shared.at.is_some() && !shared.ended)
            && stored != shared.sent as u64
        {
            let both = tf("s_ot_abweichung", &[("m", Arg::from(stored as i64)), ("n", Arg::from(shared.sent as i64))]);
            ui.label(RichText::new(both).small().color(crate::color(scheme.sub)));
        }
    }
    if let Some((org, end)) = opened {
        state.card = Some(if end {
            Card::End(org, thing)
        } else {
            match source {
                Source::Stock(rows, data) => Card::Stock(org, stock_share(rows, data)),
                Source::Blueprints(build) => Card::Blueprints(org, build()),
            }
        });
    }
    // The card of the other page (stock or blueprints) does not show here.
    let mine = match &state.card {
        Some(Card::Stock(..)) => thing == Thing::Stock,
        Some(Card::Blueprints(..)) => thing == Thing::Blueprints,
        Some(Card::End(_, which)) => *which == thing,
        None => false,
    };
    if mine {
        ui.add_space(GAP);
        card(ui, scheme, state, link, now);
        ui.add_space(GAP);
    }
    if let Some((text, since)) = state.notice.clone()
        && !widgets::notice(&ctx, scheme, since, "check", &text, None).shown
    {
        state.notice = None;
    }
}

fn line_text(share: &StockShare) -> Vec<String> {
    share
        .items
        .iter()
        .take(PREVIEW_ROWS)
        .map(|item| {
            let mut text = format!("{} · {}", item.item, amount_text(item.quantity, item.unit));
            if let Some(quality) = item.quality {
                text.push_str(&format!(" · {} {quality}", t("s_ml_sp_q")));
            }
            if !item.location.is_empty() {
                text.push_str(&format!(" · {}", item.location));
            }
            text
        })
        .collect()
}

fn card(ui: &mut Ui, scheme: &Scheme, state: &mut OrgShareState, link: &OrgLink, now: f64) {
    let Some(open) = &state.card else { return };
    let (org, thing) = match open {
        Card::Stock(org, _) => (org.clone(), Thing::Stock),
        Card::Blueprints(org, _) => (org.clone(), Thing::Blueprints),
        Card::End(org, thing) => (org.clone(), *thing),
    };
    let running = link.hub.shared(&org.id, thing).running;
    let mut go = false;
    let mut cancel = false;
    widgets::card(ui, scheme, |ui| {
        ui.set_width(ui.available_width());
        let orga = [("orga", Arg::from(org.name.as_str()))];
        let title = match (open, thing) {
            (Card::End(..), _) => tf("s_ot_beenden_name", &orga),
            (_, Thing::Stock) => tf("s_ot_titel_bestand", &orga),
            (_, Thing::Blueprints) => tf("s_ot_titel_bp", &orga),
        };
        widgets::card_title(ui, scheme, &title);
        ui.add_space(4.0);
        ui.label(tf("s_ot_orga", &[("name", Arg::from(org.name.as_str())), ("adresse", Arg::from(org.id.as_str()))]));
        let sub = crate::color(scheme.sub);
        let mut cannot: Option<String> = None;
        let go_word;
        match open {
            Card::Stock(_, share) => {
                go_word = t("s_ot_teilen_ja");
                ui.label(tf("s_ot_zeilen", &[("n", Arg::from(share.items.len() as i64))]));
                let left = share.left;
                let total = left.unknown + left.stolen + left.empty;
                if total > 0 {
                    let text = tf(
                        "s_ot_weggelassen",
                        &[
                            ("n", Arg::from(total as i64)),
                            ("a", Arg::from(left.unknown as i64)),
                            ("b", Arg::from(left.stolen as i64)),
                            ("c", Arg::from(left.empty as i64)),
                        ],
                    );
                    ui.label(RichText::new(text).color(sub));
                }
                if left.shortened > 0 {
                    ui.label(RichText::new(tf("s_ot_gekuerzt", &[("n", Arg::from(left.shortened as i64))])).color(sub));
                }
                if share.merged > 0 {
                    ui.label(RichText::new(tf("s_ot_zusammen", &[("n", Arg::from(share.merged as i64))])).color(sub));
                }
                ui.add_space(4.0);
                ui.label(t("s_ot_ersetzt"));
                ui.label(RichText::new(t("s_ot_nie")).color(sub));
                if let Some(n) = share.too_many {
                    let text = tf("s_ot_zu_viele", &[("n", Arg::from(n as i64)), ("max", Arg::from(STOCK_MAX as i64))]);
                    ui.label(RichText::new(&text).color(crate::color(scheme.yellow)));
                    cannot = Some(text);
                } else if share.items.is_empty() {
                    ui.label(RichText::new(t("s_ot_leer")).color(crate::color(scheme.yellow)));
                    cannot = Some(t("s_ot_leer"));
                }
                if !share.items.is_empty() {
                    ui.add_space(4.0);
                    widgets::collapsing(&t("s_ot_ansehen")).id_salt("org-teilen-zeilen").show(ui, |ui| {
                        for text in line_text(share) {
                            ui.label(text);
                        }
                        if share.items.len() > PREVIEW_ROWS {
                            let more = tf("s_af_mehr", &[("n", Arg::from((share.items.len() - PREVIEW_ROWS) as i64))]);
                            ui.label(RichText::new(more).color(sub));
                        }
                    });
                }
            }
            Card::Blueprints(_, share) => {
                go_word = t("s_ot_teilen_ja");
                ui.label(tf("s_ot_namen", &[("n", Arg::from(share.names.len() as i64))]));
                if share.unknown > 0 {
                    let text = tf("s_ot_unbekannt_bp", &[("n", Arg::from(share.unknown as i64))]);
                    ui.label(RichText::new(text).color(sub));
                }
                if share.twice + share.empty > 0 {
                    let text = tf("s_ot_doppelt", &[("n", Arg::from((share.twice + share.empty) as i64))]);
                    ui.label(RichText::new(text).color(sub));
                }
                ui.add_space(4.0);
                ui.label(t("s_ot_ersetzt"));
                ui.label(RichText::new(t("s_ot_nie_bp")).color(sub));
                if let Some(n) = share.too_many {
                    let text =
                        tf("s_ot_zu_viele", &[("n", Arg::from(n as i64)), ("max", Arg::from(BLUEPRINTS_MAX as i64))]);
                    ui.label(RichText::new(&text).color(crate::color(scheme.yellow)));
                    cannot = Some(text);
                } else if share.names.is_empty() {
                    ui.label(RichText::new(t("s_ot_leer")).color(crate::color(scheme.yellow)));
                    cannot = Some(t("s_ot_leer"));
                }
                if !share.names.is_empty() {
                    ui.add_space(4.0);
                    widgets::collapsing(&t("s_ot_ansehen")).id_salt("org-teilen-namen").show(ui, |ui| {
                        for name in share.names.iter().take(PREVIEW_ROWS) {
                            ui.label(name);
                        }
                    });
                }
            }
            Card::End(_, thing) => {
                go_word = t("s_ot_beenden");
                let key = if *thing == Thing::Stock { "s_ot_beenden_frage" } else { "s_ot_beenden_frage_bp" };
                ui.label(t(key));
            }
        }
        ui.add_space(6.0);
        let busy = t("s_os_laeuft");
        let reason = if running { Some(busy.as_str()) } else { None };
        let kind = if matches!(open, Card::End(..)) { ButtonKind::Danger } else { ButtonKind::Strong };
        ui.horizontal_wrapped(|row| {
            let button = widgets::Button::new(&go_word, kind, scheme);
            let blocked = cannot.as_deref().or(reason);
            if row.add(button.blocked(blocked)).clicked() {
                go = true;
            }
            let word = t("s_os_abbrechen");
            if row.add(widgets::Button::new(&word, ButtonKind::Normal, scheme).blocked(reason)).clicked() {
                cancel = true;
            }
        });
    });
    if cancel {
        state.card = None;
    }
    if go {
        send(state, link, now);
    }
}

/// Sends what the open card holds, once, and closes the card.
fn send(state: &mut OrgShareState, link: &OrgLink, now: f64) {
    let Some(open) = state.card.take() else { return };
    if link.sample {
        state.notice = Some((t("s_ot_vorschau"), now));
        return;
    }
    match open {
        Card::Stock(org, share) => {
            link.hub.share_stock(&org.id, &share);
        }
        Card::Blueprints(org, share) => {
            link.hub.share_blueprints(&org.id, &share);
        }
        Card::End(org, thing) => {
            link.hub.end_share(&org.id, thing);
        }
    }
}

#[cfg(test)]
mod tests;
