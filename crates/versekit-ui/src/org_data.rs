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

//! What the pages that show or share data of an org have in common: the
//! handle on the hub and the switches ([`OrgLink`]), noticing that a page was
//! just opened ([`OpenWatch`]) and the line above the data with its age, its
//! button and what went wrong ([`status_line`]).
//!
//! Nothing here fetches on a clock: a page asks for data when it opens and
//! when the member clicks the button.

use egui::{RichText, Ui};
use versekit_core::language::{Arg, t, tf};
use versekit_core::orgstd::areas::Areas;
use versekit_core::orgstd::hub::{ConnectionView, Hub, Kind, MAX_AGE, OrgData, SlotInfo};
use versekit_core::orgstd::{E_GONE, E_OFFLINE, E_RATE, E_RELOGIN, E_SCOPE, OrgError};
use versekit_core::theme::Scheme;

use crate::color;
use crate::widgets::{self, ButtonKind};

/// The hub and the switches of the orgs, as the pages hold them.
#[derive(Clone)]
pub struct OrgLink {
    pub hub: Hub,
    pub areas: Areas,
    /// The preview with made-up orgs: nothing is fetched or sent.
    pub sample: bool,
}

impl OrgLink {
    /// The orgs whose area `kind` shows: switch on, right granted, connection standing.
    pub fn orgs(&self, kind: Kind) -> Vec<ConnectionView> {
        self.hub.view().orgs_with(&self.areas, kind).cloned().collect()
    }

    /// Whether the area `kind` shows for any org.
    pub fn visible(&self, kind: Kind) -> bool {
        self.hub.view().area_visible(&self.areas, None, kind)
    }

    /// The orgs the member granted the right to share the own list to, for the area `kind`.
    pub fn writable(&self, write_scope: &str) -> Vec<ConnectionView> {
        self.hub.view().connections.iter().filter(|c| c.scopes.iter().any(|s| s == write_scope)).cloned().collect()
    }

    /// Asks for the list of `org` if the page was just opened and the list is old or missing.
    pub fn open(&self, org: &str, kind: Kind) {
        if !self.sample {
            self.hub.ensure(org, kind, MAX_AGE);
        }
    }
}

/// Notices that a page was just opened: it was not drawn in the frame before.
#[derive(Debug, Default)]
pub struct OpenWatch(Option<u64>);

impl OpenWatch {
    /// Whether this frame is the first of the page since it was left or never drawn.
    pub fn opened(&mut self, ctx: &egui::Context) -> bool {
        let frame = ctx.cumulative_frame_nr();
        let fresh = self.0.is_none_or(|last| frame > last + 1);
        self.0 = Some(frame);
        fresh
    }
}

/// The text of what went wrong with a list, and whether it asks the member to check the connection.
pub fn problem_text(error: OrgError, pause: u64) -> (String, bool) {
    match error.code {
        E_SCOPE => (t("s_od_f_recht"), true),
        E_RELOGIN => (t("s_od_f_neu"), false),
        E_OFFLINE => (t("s_vk_kein_netz_aus"), false),
        E_RATE => (tf("s_od_f_zu_oft", &[("s", Arg::from(pause as i64))]), false),
        E_GONE => (t("s_os_f_weg"), false),
        _ => (crate::org_connect_page::error_text(error), false),
    }
}

/// What the line above a list asks the page to do.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StatusActs {
    /// The button "Update" was clicked.
    pub refresh: bool,
    /// The button "Check connection" was clicked.
    pub check: bool,
}

fn when(seconds: u64) -> String {
    versekit_core::formats::date_time(versekit_core::language::current(), seconds as f64)
}

/// The line above the data of one org: "as of …", the org, the button to update; below it what went wrong with
/// the last fetch, in words, with the button to check the connection when a right is missing. The older rows stay
/// below it.
pub fn status_line(
    ui: &mut Ui,
    scheme: &Scheme,
    org: &ConnectionView,
    info: SlotInfo,
    now: u64,
    sample: bool,
) -> StatusActs {
    let mut acts = StatusActs::default();
    let pause = info.retry_at.saturating_sub(now);
    ui.horizontal_wrapped(|row| {
        let stand = match info.fetched_at {
            Some(at) => tf("s_od_stand", &[("zeit", Arg::from(when(at))), ("orga", Arg::from(org.name.as_str()))]),
            None => tf("s_od_noch_nie", &[("orga", Arg::from(org.name.as_str()))]),
        };
        row.label(RichText::new(stand).small().color(color(scheme.sub)));
        let word = t("s_od_aktualisieren");
        let name = tf("s_od_aktualisieren_name", &[("orga", Arg::from(org.name.as_str()))]);
        let reason = if info.loading {
            Some(t("s_os_laeuft"))
        } else if pause > 0 && !sample {
            Some(tf("s_od_pause", &[("s", Arg::from(pause as i64))]))
        } else {
            None
        };
        let button = widgets::Button::new(&word, ButtonKind::Strong, scheme).icon("rotate-cw").spoken(&name);
        if row.add(button.blocked(reason.as_deref())).clicked() {
            acts.refresh = true;
        }
    });
    if info.loading {
        widgets::loading(ui, scheme, &t("s_os_laeuft"));
    }
    if let Some(error) = info.error {
        let (text, check) = problem_text(error, pause);
        ui.horizontal_wrapped(|row| {
            row.label(RichText::new(text).small().color(color(scheme.yellow)));
            if check {
                let word = t("s_os_pruefen");
                let name = tf("s_os_pruefen_name", &[("org", Arg::from(org.name.as_str()))]);
                if row
                    .add(widgets::Button::new(&word, ButtonKind::Normal, scheme).icon("rotate-cw").spoken(&name))
                    .clicked()
                {
                    acts.check = true;
                }
            }
        });
    }
    if info.dropped > 0 {
        ui.label(
            RichText::new(tf("s_od_verworfen", &[("n", Arg::from(info.dropped as i64))]))
                .small()
                .color(color(scheme.sub)),
        );
    }
    acts
}

/// What the orgs share of one material or good, summed over all members of the orgs that show.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OrgHas {
    pub quantity: f64,
    pub unit: versekit_core::orgstd::data::Unit,
    /// The lowest quality among the rows that know one.
    pub quality_from: Option<u16>,
}

/// The shared stock by material, to show next to the own stock; never subtracted from what is missing.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StockIndex(std::collections::HashMap<String, OrgHas>);

impl StockIndex {
    /// The sums of `lots`, by the name of the material as the program compares them.
    pub fn from_lots<'a>(lots: impl IntoIterator<Item = &'a versekit_core::orgstd::data::OrgLot>) -> StockIndex {
        let mut by: std::collections::HashMap<String, OrgHas> = std::collections::HashMap::new();
        for lot in lots {
            let key = versekit_core::stock_data::norm_material(&lot.item);
            if key.is_empty() || lot.quantity <= 0.0 {
                continue;
            }
            let has = by.entry(key).or_insert(OrgHas { quantity: 0.0, unit: lot.unit, quality_from: None });
            // Rows of another unit than the first one are not added: the sum would mean nothing.
            if has.unit != lot.unit {
                continue;
            }
            has.quantity += lot.quantity;
            has.quality_from = match (has.quality_from, lot.quality) {
                (Some(a), Some(b)) => Some(a.min(b)),
                (a, b) => a.or(b),
            };
        }
        StockIndex(by)
    }

    /// What the orgs share of `material`; `None` when nothing is known: then nothing is said, not "none".
    pub fn of(&self, material: &str) -> Option<&OrgHas> {
        self.0.get(&versekit_core::stock_data::norm_material(material))
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// Who holds which blueprint in the orgs that show, by the key of the name as the program compares them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BlueprintIndex(std::collections::HashMap<String, Vec<String>>);

impl BlueprintIndex {
    /// The holders of `rows`, each name once per blueprint.
    pub fn from_rows<'a>(
        rows: impl IntoIterator<Item = &'a versekit_core::orgstd::data::OrgBlueprint>,
    ) -> BlueprintIndex {
        let mut by: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();
        for row in rows {
            let holders = by.entry(versekit_core::paths::name_key(&row.name)).or_default();
            for holder in &row.holders {
                if !holders.contains(holder) {
                    holders.push(holder.clone());
                }
            }
        }
        BlueprintIndex(by)
    }

    /// The members of the orgs that hold the blueprint with this key; `None` when nobody is known to: then nothing
    /// is said, not "nobody".
    pub fn holders(&self, key: &str) -> Option<&[String]> {
        self.0.get(key).filter(|h| !h.is_empty()).map(Vec::as_slice)
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// The indexes of a frame, built again only when the data or the switches changed.
#[derive(Default)]
pub struct IndexCache {
    key: Option<(u64, bool, bool)>,
    stock: std::sync::Arc<StockIndex>,
    blueprints: std::sync::Arc<BlueprintIndex>,
}

impl OrgLink {
    /// The shared stock by material and the holders by blueprint of the orgs whose area shows; each empty without
    /// such an org.
    pub fn indexes(&self, cache: &mut IndexCache) -> (std::sync::Arc<StockIndex>, std::sync::Arc<BlueprintIndex>) {
        let view = self.hub.view();
        let stock_shown = view.area_visible(&self.areas, None, Kind::Stock);
        let blueprints_shown = view.area_visible(&self.areas, None, Kind::Blueprints);
        if cache.key != Some((view.generation, stock_shown, blueprints_shown)) {
            let stock = if stock_shown {
                let lots: Vec<_> =
                    view.orgs_with(&self.areas, Kind::Stock).map(|org| self.hub.data(&org.id).stock.rows).collect();
                StockIndex::from_lots(lots.iter().flat_map(|rows| rows.iter()))
            } else {
                StockIndex::default()
            };
            let blueprints = if blueprints_shown {
                let rows: Vec<_> = view
                    .orgs_with(&self.areas, Kind::Blueprints)
                    .map(|org| self.hub.data(&org.id).blueprints.rows)
                    .collect();
                BlueprintIndex::from_rows(rows.iter().flat_map(|rows| rows.iter()))
            } else {
                BlueprintIndex::default()
            };
            *cache = IndexCache {
                key: Some((view.generation, stock_shown, blueprints_shown)),
                stock: std::sync::Arc::new(stock),
                blueprints: std::sync::Arc::new(blueprints),
            };
        }
        (cache.stock.clone(), cache.blueprints.clone())
    }
}

/// Rows of an org for the tests of the pages that only receive the indexes of the window.
#[cfg(test)]
pub(crate) mod made_up {
    use versekit_core::orgstd::data::{OrgBlueprint, OrgLot, Unit};

    /// `quantity` SCU of `item` at quality 500, shared by nobody in particular.
    pub fn lot(item: &str, quantity: f64, quality: u16) -> OrgLot {
        OrgLot {
            item: item.into(),
            quantity,
            unit: Unit::Scu,
            quality: Some(quality),
            location: String::new(),
            member: String::new(),
        }
    }

    pub fn blueprint(name: &str, holders: &[&str]) -> OrgBlueprint {
        OrgBlueprint { name: name.into(), holders: holders.iter().map(|h| (*h).to_string()).collect() }
    }
}

/// The line "In the org stock: …" under the own stock of a material.
pub fn org_has_text(has: &OrgHas) -> String {
    let amount = amount_text(has.quantity, has.unit);
    match has.quality_from {
        Some(quality) => tf("s_od_im_lager", &[("menge", Arg::from(amount)), ("q", Arg::from(i64::from(quality)))]),
        None => tf("s_od_im_lager_ohne", &[("menge", Arg::from(amount))]),
    }
}

/// The space between two columns of a list.
const COLUMN_GAP: f32 = 4.0;

/// Runs `add` once for each column, each in a cell as wide as its least width plus its share of what is left; the
/// line is as high as its highest cell, so the heads and every row stand under each other. A wide content never
/// pushes the line wider than the page: each cell is clipped to its width.
pub fn columns_line(ui: &mut Ui, least: &[f32], shares: &[f32], add: impl FnMut(&mut Ui, usize)) {
    columns_line_in(ui, least, shares, add);
}

/// [`columns_line`] that also gives the place and the id of the whole line, for a line that is a click area.
pub fn columns_line_in(
    ui: &mut Ui,
    least: &[f32],
    shares: &[f32],
    mut add: impl FnMut(&mut Ui, usize),
) -> (egui::Id, egui::Rect) {
    let whole = ui.available_width();
    let usable = (whole - COLUMN_GAP * (least.len() - 1) as f32).max(0.0);
    let spare = (usable - least.iter().sum::<f32>()).max(0.0);
    let origin = ui.cursor().min;
    let clip = ui.clip_rect();
    let mut left = origin.x;
    let mut height = 0.0_f32;
    for (at, (least, share)) in least.iter().zip(shares).enumerate() {
        let width = least + spare * share;
        let place = egui::Rect::from_min_size(egui::pos2(left, origin.y), egui::vec2(width, 10_000.0));
        let mut cell =
            ui.new_child(egui::UiBuilder::new().max_rect(place).layout(egui::Layout::top_down(egui::Align::Min)));
        cell.set_clip_rect(clip.intersect(place));
        add(&mut cell, at);
        height = height.max(cell.min_rect().height());
        left += width + COLUMN_GAP;
    }
    let (id, rect) = ui.allocate_space(egui::vec2(whole, height));
    (id, egui::Rect::from_min_size(rect.min, egui::vec2(whole, height)))
}

/// The text of an amount with its unit.
pub fn amount_text(quantity: f64, unit: versekit_core::orgstd::data::Unit) -> String {
    use versekit_core::orgstd::data::Unit;
    let word = match unit {
        Unit::Scu => "SCU".to_string(),
        Unit::Cscu => "cSCU".to_string(),
        Unit::Piece => t("s_lg_stueck"),
    };
    versekit_core::formats::amount(versekit_core::language::current(), quantity, &word)
}

/// Made-up data of the made-up org of the preview.
pub fn sample_data(now: u64) -> OrgData {
    use std::sync::Arc;
    use versekit_core::orgstd::data::{OrderLine, OrderStatus, OrgBlueprint, OrgLot, OrgOrder, Unit};
    use versekit_core::orgstd::hub::Slot;
    let lot = |item: &str, quantity: f64, unit: Unit, quality: Option<u16>, location: &str, member: &str| OrgLot {
        item: item.into(),
        quantity,
        unit,
        quality,
        location: location.into(),
        member: versekit_core::streamer::private(member.into()),
    };
    let stock = vec![
        lot("Laranite", 42.5, Unit::Scu, Some(812), "Area18", "Vega"),
        lot("Laranite", 18.0, Unit::Scu, Some(640), "Levski", "Mira"),
        lot("Agricium", 120.0, Unit::Scu, Some(701), "Area18", "Vega"),
        lot("Hadanite", 12.0, Unit::Piece, Some(955), "Orison", "Rook"),
        lot("Construction Materials", 64.0, Unit::Scu, None, "Seraphim Station", "Mira"),
        lot("Venture Arms", 2.0, Unit::Piece, None, "", "Rook"),
    ];
    let blueprints = vec![
        OrgBlueprint {
            name: "Venture Arms".into(),
            holders: ["Vega", "Mira"].map(|m| versekit_core::streamer::private(m.into())).to_vec(),
        },
        OrgBlueprint { name: "P4-AR Rifle".into(), holders: vec![versekit_core::streamer::private("Rook".into())] },
    ];
    let order = |id: &str, title: &str, status: OrderStatus, lines: Vec<OrderLine>| OrgOrder {
        id: id.into(),
        title: title.into(),
        status,
        location: "Area18".into(),
        due: "2026-12-24".into(),
        note: String::new(),
        lines,
    };
    let line = |item: &str, quantity: f64, delivered: f64| OrderLine {
        item: item.into(),
        quantity,
        unit: Unit::Scu,
        delivered,
    };
    let orders = vec![
        order("a1", "Laranite", OrderStatus::Open, vec![line("Laranite", 100.0, 40.0), line("Agricium", 50.0, 50.0)]),
        order("a2", "Quantanium", OrderStatus::Done, vec![line("Construction Materials", 64.0, 64.0)]),
    ];
    let fetched = Some(now);
    OrgData {
        stock: Slot { rows: Arc::new(stock), fetched_at: fetched, ..Slot::default() },
        blueprints: Slot { rows: Arc::new(blueprints), fetched_at: fetched, ..Slot::default() },
        orders: Slot { rows: Arc::new(orders), fetched_at: fetched, ..Slot::default() },
    }
}
