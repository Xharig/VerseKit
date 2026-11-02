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

//! The page "Unit demand" (`pages._demand` of 4.12): what the open orders of
//! the own units in the Basetool still need, weighed against the own stock.
//!
//! Materials with the lowest quality and the open amount, next to them what
//! lies in the own stock; items, marked when the own blueprints cover them;
//! for a logistician the orders one by one. The data lives in memory only
//! (see `versekit_core::basetool::demand`).

use std::sync::Arc;
use std::time::SystemTime;

use egui::{RichText, Ui};
use versekit_core::basetool::connection::Clock;
use versekit_core::basetool::demand::{Demand, DemandStore, Material, Order};
use versekit_core::basetool::sync::{Phase, SyncHandle};
use versekit_core::formats;
use versekit_core::jsonio::Value;
use versekit_core::language::{self, Arg, t, tf};
use versekit_core::paths::Paths;
use versekit_core::stock;
use versekit_core::stock_use;
use versekit_core::theme::Scheme;

use crate::color;
use crate::widgets::{self, ButtonKind};

const LOOK_SECONDS: f64 = 1.0;
/// So many lines of an order, the rest is counted.
const ORDER_LINES: usize = 6;
/// So many ores of a material are named, the rest is counted.
const ORES: usize = 4;
/// So long a comment stays in the list.
const COMMENT_LENGTH: usize = 200;

/// What the page needs from the sync.
#[derive(Clone)]
pub struct Link {
    pub store: Arc<DemandStore>,
    pub sync: SyncHandle,
    pub clock: Clock,
}

impl Link {
    /// The page on the sync of the program.
    pub fn from_wire(wire: &crate::basetool_page::Wire) -> Link {
        Link {
            store: wire.demand.clone(),
            sync: wire.sync.clone(),
            clock: versekit_core::basetool::connection::system_clock(),
        }
    }
}

/// What the page holds.
pub struct DemandState {
    link: Option<Link>,
    paths: Option<Paths>,
    stock: Vec<Value>,
    stock_seen: Option<SystemTime>,
    looked_at: f64,
    /// The demand shown and the generation it came with.
    data: Option<Arc<Demand>>,
    seen: u64,
    /// The reason the Basetool gave for showing nothing.
    reason: String,
    /// "Compare now" was pressed and no pass has ended since.
    asked: Option<u64>,
}

impl Default for DemandState {
    /// The sample: a made-up demand against a made-up stock.
    fn default() -> Self {
        let demand =
            versekit_core::basetool::demand::parse(&versekit_core::jsonio::loads(SAMPLE).unwrap_or(Value::Null));
        let stock = match versekit_core::jsonio::loads(SAMPLE_STOCK) {
            Ok(Value::Array(items)) => items,
            _ => Vec::new(),
        };
        DemandState {
            link: None,
            paths: None,
            stock,
            stock_seen: None,
            looked_at: f64::NEG_INFINITY,
            data: Some(Arc::new(demand)),
            seen: 0,
            reason: String::new(),
            asked: None,
        }
    }
}

const SAMPLE: &str = r#"{"materials": [
    {"material": {"name": "Laranite"}, "openQuantity": {"amount": 18.5, "unit": "SCU"}, "minQuality": 600,
     "rawRefs": [{"name": "Laranite (Raw)"}]},
    {"material": {"name": "Agricium"}, "openQuantity": {"amount": 40, "unit": "SCU"}, "minQuality": 0}],
  "items": [{"item": {"name": "Arc Shield"}, "openQuantity": {"amount": 2, "unit": "PIECE"}, "craftableByMe": true},
            {"item": {"name": "Grand Core"}, "openQuantity": {"amount": 1, "unit": "PIECE"}, "craftableByMe": false}]}"#;

const SAMPLE_STOCK: &str = r#"[{"material":"Laranite","menge":6.0,"qualitaet":700,"ort":"Levski"}]"#;

fn file_time(file: &str) -> Option<SystemTime> {
    std::fs::metadata(file).and_then(|m| m.modified()).ok()
}

impl DemandState {
    /// On the sync of the program and, where there are own files, their stock.
    pub fn new(link: Option<Link>, paths: Option<&Paths>) -> DemandState {
        let mut state = DemandState {
            link,
            paths: paths.map(|p| Paths::new(p.host.clone())),
            stock: Vec::new(),
            stock_seen: None,
            looked_at: f64::NEG_INFINITY,
            data: None,
            seen: 0,
            reason: String::new(),
            asked: None,
        };
        if state.link.is_none() {
            state.data = DemandState::default().data;
            state.stock = DemandState::default().stock;
        }
        state.reload_stock();
        state
    }

    /// The code of the error the last pass ended with, if it ended with one.
    fn failure(&self) -> Option<String> {
        let status = self.link.as_ref()?.sync.status().get().1?;
        (status.phase == Phase::Error && !status.code.is_empty()).then(|| status.code.clone())
    }

    /// When the demand shown was fetched (seconds since 1970).
    fn fetched_at(&self) -> Option<f64> {
        self.link.as_ref().and_then(|l| l.store.fetched_at())
    }

    fn reload_stock(&mut self) {
        let Some(paths) = &self.paths else { return };
        let file = paths.app_file(stock::RAW_FILE);
        self.stock_seen = file_time(&file);
        self.stock = stock::load(&file, "materials.load");
    }

    /// Takes in what the sync brought and reads the stock again when it changed.
    fn follow(&mut self, ui: &Ui) {
        if let Some(link) = &self.link {
            let now = (link.clock)();
            let (generation, data) = link.store.current(now);
            if generation != self.seen {
                self.seen = generation;
                self.data = data;
                self.reason = link.store.reason();
                self.asked = None;
            }
            // A pass that ended without bringing data (an error) also ends the wait.
            if let Some(since) = self.asked {
                let status = link.sync.status();
                if status.generation() > since && status.get().1.is_none_or(|s| s.phase != Phase::Running) {
                    self.asked = None;
                }
            }
        }
        let time = ui.input(|i| i.time);
        if time - self.looked_at < LOOK_SECONDS {
            return;
        }
        self.looked_at = time;
        if let Some(paths) = &self.paths
            && file_time(&paths.app_file(stock::RAW_FILE)) != self.stock_seen
        {
            self.reload_stock();
        }
    }
}

fn tn(key: &str, args: &[Arg]) -> String {
    let named: Vec<(String, Arg)> = args.iter().enumerate().map(|(i, a)| (i.to_string(), a.clone())).collect();
    let refs: Vec<(&str, Arg)> = named.iter().map(|(n, a)| (n.as_str(), a.clone())).collect();
    tf(key, &refs)
}

fn sub(scheme: &Scheme, text: impl Into<String>) -> RichText {
    RichText::new(text.into()).small().color(color(scheme.sub))
}

fn is_piece(unit: &str) -> bool {
    unit.eq_ignore_ascii_case("PIECE")
}

fn unit_word(piece: bool) -> String {
    if piece { t("s_lg_stueck") } else { "SCU".to_string() }
}

/// An amount without its unit: SCU with up to three places, pieces whole.
fn amount_text(value: f64) -> String {
    formats::amount(language::current(), value, "")
}

/// One line of the list: a left part, a right part and a small line below.
fn row(ui: &mut Ui, scheme: &Scheme, k: usize, left: (&str, bool), right: &str, below: &str) {
    let frame = egui::Frame::new()
        .fill(if k % 2 == 1 { color(scheme.bar) } else { egui::Color32::TRANSPARENT })
        .inner_margin(egui::Margin::symmetric(8, 4));
    frame.show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal_wrapped(|top| {
            let name = RichText::new(left.0).color(color(if left.1 { scheme.accent } else { scheme.fg }));
            top.label(name);
            if !right.is_empty() {
                top.label(sub(scheme, right));
            }
        });
        if !below.is_empty() {
            ui.label(sub(scheme, below));
        }
    });
}

/// A state that stays on the page: icon, bold sentence and a muted text, both wrapped (the
/// box of the widgets cuts long texts, which these sentences are).
fn state_box(ui: &mut Ui, scheme: &Scheme, tone: versekit_core::theme::Color, head: &str, text: &str) {
    egui::Frame::new()
        .fill(color(scheme.surface))
        .stroke(egui::Stroke::new(1.0, color(tone)))
        .inner_margin(egui::Margin::same(10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_wrapped(|row| {
                if let Some(image) = crate::icons::image("shield-alert", tone, 18.0) {
                    row.add(image);
                }
                row.label(widgets::bold(row, head, egui::TextStyle::Body));
            });
            ui.label(sub(scheme, text));
        });
}

fn material_rows(ui: &mut Ui, scheme: &Scheme, state: &DemandState, data: &Demand) {
    if data.materials.is_empty() {
        widgets::empty_state(ui, scheme, "scan-search", &t("s_bd_nichts"));
        return;
    }
    widgets::list_frame(ui, scheme, |list| {
        list.set_width(list.available_width());
        for (k, line) in data.materials.iter().enumerate() {
            material_row(list, scheme, k, line, &state.stock);
        }
    });
}

fn material_row(ui: &mut Ui, scheme: &Scheme, k: usize, line: &Material, stock_rows: &[Value]) {
    let piece = is_piece(&line.unit);
    let unit = unit_word(piece);
    let (have, _too_low) = stock_use::amount_with_quality(stock_rows, &line.name, line.min_quality as f64);
    let mut right = tn("s_bd_offen", &[Arg::from(amount_text(line.amount)), Arg::from(unit.as_str())]);
    if have > 0.0 {
        right.push_str("  ·  ");
        right.push_str(&tn("s_bd_habe", &[Arg::from(format!("{} {unit}", amount_text(have)))]));
    }
    let mut parts = Vec::new();
    if line.min_quality != 0 {
        parts.push(tn("s_bd_ab_q", &[Arg::Int(line.min_quality)]));
    }
    if !line.raw.is_empty() {
        let mut ores = line.raw.iter().take(ORES).cloned().collect::<Vec<_>>().join(", ");
        if line.raw.len() > ORES {
            ores.push_str(&format!(" +{}", line.raw.len() - ORES));
        }
        parts.push(tn("s_bd_aus", &[Arg::from(ores)]));
    }
    row(ui, scheme, k, (&line.name, have > 0.0), &right, &parts.join("  ·  "));
}

fn order_rows(ui: &mut Ui, scheme: &Scheme, orders: &[Order]) {
    ui.add_space(8.0);
    ui.label(widgets::bold(ui, t("s_bd_auftraege"), egui::TextStyle::Body));
    ui.label(sub(scheme, t("s_bd_auftraege_h")));
    if orders.is_empty() {
        widgets::empty_state(ui, scheme, "scan-search", &t("s_bd_keine_auftraege"));
        return;
    }
    widgets::list_frame(ui, scheme, |list| {
        list.set_width(list.available_width());
        for (k, order) in orders.iter().enumerate() {
            order_row(list, scheme, k, order);
        }
    });
}

fn order_row(ui: &mut Ui, scheme: &Scheme, k: usize, order: &Order) {
    let mut head = match order.number {
        Some(number) => format!("#{number}"),
        None => t("s_bd_auftrag"),
    };
    if !order.handle.is_empty() {
        head.push_str("  ·  ");
        head.push_str(&order.handle);
    }
    let mut right = match order.status.as_str() {
        "OPEN" => t("s_bd_status_open"),
        "IN_PROGRESS" => t("s_bd_status_in_progress"),
        other => other.to_string(),
    };
    if let Some(priority) = order.priority {
        right.push_str("  ·  ");
        right.push_str(&tn("s_bd_prio", &[Arg::Int(priority)]));
    }
    let mut parts = Vec::new();
    if !order.requesting.is_empty() {
        parts.push(tn("s_bd_fuer", &[Arg::from(order.requesting.as_str())]));
    }
    for line in order.lines.iter().take(ORDER_LINES) {
        let piece = is_piece(&line.unit);
        let mut text = format!("{} {} {}", amount_text(line.amount), unit_word(piece), line.name);
        if line.min_quality != 0 {
            text.push_str(&format!(" ({})", tn("s_bd_ab_q", &[Arg::Int(line.min_quality)])));
        }
        parts.push(text);
    }
    if order.lines.len() > ORDER_LINES {
        parts.push(tn("s_bt_und_mehr", &[Arg::from(order.lines.len() - ORDER_LINES)]));
    }
    if !order.comment.is_empty() {
        let mut comment: String = order.comment.chars().take(COMMENT_LENGTH).collect();
        if order.comment.chars().count() > COMMENT_LENGTH {
            comment.push('…');
        }
        parts.push(tf("s_p9_kommentar", &[("0", Arg::from(comment))]));
    }
    row(ui, scheme, k, (&head, false), &right, &parts.join("\n"));
}

/// Draws the page.
pub fn show_page(ui: &mut Ui, scheme: &Scheme, state: &mut DemandState) {
    state.follow(ui);
    if state.link.is_some() {
        // The sync wakes the window; a look every second also notices a stock written elsewhere.
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(1000));
    }
    ui.label(sub(scheme, t("s_bd_lead")));
    ui.add_space(6.0);
    if !state.reason.is_empty() {
        state_box(ui, scheme, scheme.yellow, &t("s_p9_bedarf_kein_recht_kopf"), &t("s_p9_bedarf_kein_recht"));
        return;
    }
    let Some(data) = state.data.clone() else {
        ui.label(sub(scheme, t("s_bd_warten")));
        ui.add_space(6.0);
        if let Some(code) = state.failure() {
            ui.label(RichText::new(versekit_core::basetool::error::error_text(&code)).small().color(color(scheme.red)));
        }
        if state.asked.is_some() {
            widgets::loading(ui, scheme, &t("s_bt_laeuft"));
        } else if let Some(link) = &state.link {
            let word = t("s_bt_jetzt");
            if ui.add(widgets::Button::new(&word, ButtonKind::Normal, scheme).icon("rotate-cw")).clicked() {
                state.asked = Some(link.sync.status().generation());
                link.sync.request_now();
            }
        }
        return;
    };
    if let Some(at) = state.fetched_at() {
        let now = state.link.as_ref().map_or(at, |l| (l.clock)());
        let exact = formats::date_time(language::current(), at);
        widgets::data_age(ui, scheme, (now - at).max(0.0), &exact);
    }
    ui.label(widgets::bold(ui, t("s_bd_rohstoffe"), egui::TextStyle::Body));
    material_rows(ui, scheme, state, &data);
    ui.add_space(8.0);
    ui.label(widgets::bold(ui, t("s_bd_gegenstaende"), egui::TextStyle::Body));
    if data.items.is_empty() {
        widgets::empty_state(ui, scheme, "scan-search", &t("s_bd_nichts"));
    } else {
        widgets::list_frame(ui, scheme, |list| {
            list.set_width(list.available_width());
            for (k, item) in data.items.iter().enumerate() {
                let piece = is_piece(&item.unit) || item.unit.is_empty();
                let right = tn("s_bd_offen", &[Arg::from(amount_text(item.amount)), Arg::from(unit_word(piece))]);
                let below = if item.craftable { t("s_bd_kannst_bauen") } else { String::new() };
                row(list, scheme, k, (&item.name, item.craftable), &right, &below);
            }
        });
    }
    if let Some(orders) = &data.orders {
        order_rows(ui, scheme, orders);
    }
    ui.add_space(10.0);
    ui.label(sub(scheme, t("s_bd_hinweis")));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::basetool_page::Wire;
    use egui::{Rect, vec2};
    use versekit_core::basetool::demo::DemoLogin;
    use versekit_core::basetool::scopes::SETTING_DEMAND;
    use versekit_core::language::{Choice, set_language};

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

    /// One frame of the page inside the page frame of the window: the texts and how far it sticks out.
    fn draw(ctx: &egui::Context, state: &mut DemandState, width: f32) -> (Vec<String>, f32) {
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(width, 1600.0))),
            ..Default::default()
        };
        let scheme = versekit_core::theme::current();
        let mut over = 0.0;
        let mut output = ctx.run_ui(input, |ui| {
            let shown = crate::layout::page(ui, scheme, "bedarf-test", |ui| show_page(ui, scheme, state));
            over = shown.overflow;
        });
        output.textures_delta.clear();
        (texts(&output.shapes), over)
    }

    fn context() -> egui::Context {
        let ctx = egui::Context::default();
        crate::install(&ctx);
        ctx
    }

    /// A wire on the made-up Basetool, connected, the demand switched on, one pass done.
    fn connected(name: &str) -> Wire {
        let dir = std::env::temp_dir().join(format!("versekit-bedarf-{}-{name}", versekit_core::testdir::run_tag()));
        let _ = std::fs::remove_dir_all(&dir);
        let wire = Wire::in_folder(&dir.to_string_lossy(), false);
        wire.demo.as_ref().expect("demo").script_login(DemoLogin::Approve, 1);
        assert!(wire.sync.set_setting(SETTING_DEMAND, Value::Bool(true)));
        wire.sync.connect();
        let end = std::time::Instant::now() + std::time::Duration::from_secs(20);
        while !wire.sync.connection().connected() && std::time::Instant::now() < end {
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        assert!(wire.sync.connection().connected());
        wire
    }

    #[test]
    fn the_demand_of_the_sync_is_shown_against_the_own_stock() {
        let _language = crate::language_lock();
        set_language(Choice::De);
        let wire = connected("shown");
        wire.sync.pass_now().expect("pass");
        let mut state = DemandState::new(Some(Link::from_wire(&wire)), None);
        state.stock = match versekit_core::jsonio::loads(
            r#"[{"material":"Laranite","menge":4.0,"qualitaet":700,"ort":"Levski"},
                {"material":"Laranite","menge":9.0,"qualitaet":200,"ort":"Levski"}]"#,
        ) {
            Ok(Value::Array(items)) => items,
            _ => panic!("array"),
        };
        let (shown, over) = draw(&context(), &mut state, 900.0);
        assert_eq!(over, 0.0);
        for name in ["Laranite", "Agricium", "Hadanite", "FR-66 Shield", "Bulwark Armor"] {
            assert!(shown.contains(&name.to_string()), "{name}: {shown:?}");
        }
        // Laranite wants quality 600: only the 4 of quality 700 count as in stock.
        let wanted = tn("s_bd_offen", &[Arg::from("18,5"), Arg::from("SCU")])
            + "  ·  "
            + &tn("s_bd_habe", &[Arg::from("4 SCU")]);
        assert!(shown.contains(&wanted), "{wanted}: {shown:?}");
        let below = tn("s_bd_ab_q", &[Arg::Int(600)]) + "  ·  " + &tn("s_bd_aus", &[Arg::from("Laranite (Raw)")]);
        assert!(shown.contains(&below), "{below}: {shown:?}");
        assert!(shown.contains(&t("s_bd_kannst_bauen")), "{shown:?}");
        // The pieces of the Hadanite are counted as pieces.
        assert!(shown.iter().any(|s| s.contains(&t("s_lg_stueck"))), "{shown:?}");
        // The demo login granted the right to see the orders one by one.
        assert!(shown.contains(&t("s_bd_auftraege")), "{shown:?}");
        assert!(shown.iter().any(|s| s.starts_with("#12  ·  PlayerTwo")), "{shown:?}");
        assert!(shown.contains(&t("s_bd_hinweis")), "{shown:?}");
        let _ = std::fs::remove_dir_all(
            std::env::temp_dir().join(format!("versekit-bedarf-{}-shown", versekit_core::testdir::run_tag())),
        );
    }

    #[test]
    fn without_a_demand_the_page_waits_and_the_button_asks_for_a_sync() {
        let _language = crate::language_lock();
        set_language(Choice::De);
        let wire = connected("waits");
        let mut state = DemandState::new(Some(Link::from_wire(&wire)), None);
        let ctx = context();
        let (shown, _) = draw(&ctx, &mut state, 900.0);
        assert!(shown.contains(&t("s_bd_warten")) && shown.contains(&t("s_bt_jetzt")), "{shown:?}");
        assert!(!shown.contains(&t("s_bd_rohstoffe")));
        // The sync brings the demand; the page takes it in without being told.
        wire.sync.pass_now().expect("pass");
        let (shown, _) = draw(&ctx, &mut state, 900.0);
        assert!(shown.contains(&t("s_bd_rohstoffe")) && !shown.contains(&t("s_bd_warten")), "{shown:?}");
    }

    #[test]
    fn a_reason_is_said_and_nothing_else_is_shown() {
        let _language = crate::language_lock();
        set_language(Choice::De);
        let wire = connected("reason");
        wire.demand.take(
            versekit_core::basetool::demand::parse(
                &versekit_core::jsonio::loads(r#"{"reason": "NOT_PERMITTED"}"#).expect("json"),
            ),
            wire.sync.now(),
        );
        let mut state = DemandState::new(Some(Link::from_wire(&wire)), None);
        let (shown, _) = draw(&context(), &mut state, 900.0);
        assert!(shown.contains(&t("s_p9_bedarf_kein_recht")), "{shown:?}");
        assert!(shown.contains(&t("s_p9_bedarf_kein_recht_kopf")), "{shown:?}");
        assert!(!shown.contains(&t("s_bd_rohstoffe")));
    }

    #[test]
    fn the_page_fits_in_both_languages_at_every_width() {
        let _language = crate::language_lock();
        let wire = connected("fits");
        wire.sync.pass_now().expect("pass");
        let mut findings = Vec::new();
        for lang in [Choice::De, Choice::En] {
            set_language(lang);
            for points in versekit_core::font_size::POINTS_RANGE.0..=versekit_core::font_size::POINTS_RANGE.1 {
                for width in [380.0, 520.0, 900.0] {
                    let ctx = context();
                    crate::apply_size(&ctx, points);
                    let mut state = DemandState::new(Some(Link::from_wire(&wire)), None);
                    let (_, over) = (0..3).map(|_| draw(&ctx, &mut state, width)).last().expect("frames");
                    if over > 0.5 {
                        findings.push(format!("{lang:?} Größe {points} Breite {width}: {over:.0} zu breit"));
                    }
                }
            }
        }
        set_language(Choice::De);
        assert!(findings.is_empty(), "{}", findings.join("\n"));
    }

    #[test]
    fn the_sample_shows_a_made_up_demand() {
        let _language = crate::language_lock();
        set_language(Choice::De);
        let mut state = DemandState::default();
        let (shown, over) = draw(&context(), &mut state, 900.0);
        assert_eq!(over, 0.0);
        assert!(shown.contains(&"Laranite".to_string()) && shown.contains(&"Arc Shield".to_string()), "{shown:?}");
        // What can be crafted stands first, then the rest; the other way round Grand Core would lead.
        let at = |name: &str| shown.iter().position(|s| s == name).expect(name);
        assert!(at("Arc Shield") < at("Grand Core"), "{shown:?}");
        // The materials alphabetical: Agricium before Laranite.
        assert!(at("Agricium") < at("Laranite"), "{shown:?}");
    }
}
