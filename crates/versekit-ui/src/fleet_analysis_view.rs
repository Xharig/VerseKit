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

//! The fleet analysis over the list of the hangar: a part, closed at
//! first and opened by a button in the head card of the page, that shows per archetype (combat, industry and supply, travel and
//! exploration) which gameplay loops the own ships cover. Loops without any
//! ship stand in one line "Not covered"; secondary roles are folded; below
//! come snubs without a mothership, the vehicles that come along with a ship,
//! the melt numbers, ships the data does not know and the source.
//!
//! The assignment of ships to loops is the assessment of the author of the
//! Fleetmanifest (Maluh, MIT), not game data.

use std::collections::HashSet;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use egui::{RichText, Ui};
use versekit_core::browser;
use versekit_core::fleet_analysis::{self, Analysis, Archetype, Listed, SlotResult};
use versekit_core::hangar;
use versekit_core::jsonio::Map;
use versekit_core::language::{self, Arg, Lang, t, tf};
use versekit_core::theme::Scheme;

use crate::layout::GAP;
use crate::{color, widgets};

/// What the fold keeps: whether it is open, and the last analysis with the
/// fingerprint of the hangar it was made for.
#[derive(Default)]
pub struct FleetView {
    pub open: bool,
    /// The places whose secondary roles are unfolded.
    unfolded: HashSet<String>,
    cache: Option<(u64, Arc<Analysis>)>,
}

/// Everything the analysis reads from the entries, as one number. Typing in
/// the search or switching a ship ready changes nothing here.
fn fingerprint(entries: &[&Map]) -> u64 {
    let mut hasher = DefaultHasher::new();
    entries.len().hash(&mut hasher);
    for entry in entries {
        for key in ["name", "hersteller", "herkunft", "schmelz_wahl"] {
            hangar::field_text(entry, key).hash(&mut hasher);
        }
        hangar::melt_imported(entry).hash(&mut hasher);
    }
    hasher.finish()
}

impl FleetView {
    /// The fold, open or closed.
    pub fn with_open(open: bool) -> FleetView {
        FleetView { open, ..FleetView::default() }
    }

    /// The analysis of these entries; made again only when they changed.
    fn analysis(&mut self, entries: &[&Map]) -> Arc<Analysis> {
        let print = fingerprint(entries);
        match &self.cache {
            Some((seen, analysis)) if *seen == print => Arc::clone(analysis),
            _ => {
                let analysis = Arc::new(fleet_analysis::analyse(entries));
                self.cache = Some((print, Arc::clone(&analysis)));
                analysis
            }
        }
    }
}

fn small(ui: &mut Ui, text: &str, tone: egui::Color32) {
    widgets::reading(ui, RichText::new(text).small().color(tone));
}

/// `2026-10-19` as `19.10.2026` or `19 Oct 2026`; what does not parse stays as it is.
fn shown_date(iso: &str, lang: Lang) -> String {
    const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    let parts: Vec<&str> = iso.split('-').collect();
    let [year, month, day] = parts[..] else { return iso.to_string() };
    let (Ok(month), Ok(day)) = (month.parse::<usize>(), day.parse::<u32>()) else { return iso.to_string() };
    if !(1..=12).contains(&month) {
        return iso.to_string();
    }
    match lang {
        Lang::De => format!("{day:02}.{month:02}.{year}"),
        Lang::En => format!("{day} {} {year}", MONTHS[month - 1]),
    }
}

/// How a listed ship reads: its name, and "incl." where it only comes along
/// with another ship.
fn listed_text(list: &[Listed]) -> String {
    let mark = t("s_fav_inkl_marke");
    list.iter()
        .map(|l| if l.included { format!("{} ({mark})", l.name) } else { l.name.clone() })
        .collect::<Vec<_>>()
        .join(", ")
}

/// One place: the loop with its gap mark, the ships in a primary and planned
/// role, and the secondary roles in a fold of their own.
fn slot_row(
    ui: &mut Ui,
    scheme: &Scheme,
    data: &fleet_analysis::Data,
    result: &SlotResult,
    unfolded: &mut HashSet<String>,
) {
    let lang = language::current();
    let name = result.slot.lop(data).name(lang);
    // The gap badge stands at the right edge, so it does not move with the name.
    ui.horizontal(|line| {
        line.with_layout(egui::Layout::right_to_left(egui::Align::Center), |line| {
            if result.gap() {
                let badge = egui::Frame::new()
                    .stroke(egui::Stroke::new(1.0, color(scheme.gold)))
                    .corner_radius(crate::control_radius(scheme))
                    .inner_margin(egui::Margin::symmetric(5, 1))
                    .show(line, |b| {
                        let text = RichText::new(t("s_fav_luecke")).small().color(color(scheme.gold));
                        b.add(egui::Label::new(text).sense(egui::Sense::hover()))
                    });
                badge.inner.on_hover_text(t("s_fav_luecke_hinweis"));
            }
            line.with_layout(egui::Layout::left_to_right(egui::Align::Center), |line| {
                line.add(egui::Label::new(name).wrap());
            });
        });
    });
    let tone = color(scheme.sub);
    if !result.primary.is_empty() {
        small(ui, &tf("s_fa_haupt", &[("namen", Arg::from(listed_text(&result.primary)))]), tone);
    }
    if !result.planned.is_empty() {
        small(ui, &tf("s_fa_geplant", &[("namen", Arg::from(listed_text(&result.planned)))]), tone);
    }
    if !result.secondary.is_empty() {
        let title = tf("s_fav_nebenrollen", &[("n", Arg::from(result.secondary.len()))]);
        let key = result.slot.key(data);
        let open = unfolded.contains(&key);
        let shown = widgets::collapsing(&title).id_salt(("fleet-secondary", &key)).open(Some(open)).show(ui, |ui| {
            small(ui, &listed_text(&result.secondary), tone);
        });
        if shown.header_response.clicked() && !unfolded.remove(&key) {
            unfolded.insert(key);
        }
    }
}

/// An archetype: its heading, the one line for the loops without a ship and
/// the striped list of the others.
fn archetype_section(
    ui: &mut Ui,
    scheme: &Scheme,
    analysis: &Analysis,
    archetype: Archetype,
    unfolded: &mut HashSet<String>,
) {
    let data = fleet_analysis::data();
    let lang = language::current();
    ui.add_space(GAP);
    ui.label(RichText::new(t(archetype.key())).strong());
    ui.add_space(2.0);
    let (empty, filled): (Vec<&SlotResult>, Vec<&SlotResult>) = analysis.of(archetype).partition(|r| r.empty());
    if !empty.is_empty() {
        let names: Vec<&str> = empty.iter().map(|r| r.slot.lop(data).name(lang)).collect();
        let line = format!("{} {}", t("s_fav_nicht_abgedeckt"), names.join(", "));
        ui.add(egui::Label::new(RichText::new(line).small().color(color(scheme.gold))).wrap());
        ui.add_space(4.0);
    }
    if filled.is_empty() {
        return;
    }
    widgets::list_frame(ui, scheme, |list| {
        list.set_width(list.available_width() - crate::layout::SAFETY);
        for (k, result) in filled.iter().enumerate() {
            let stripe = list.painter().add(egui::Shape::Noop);
            let row = list.vertical(|cell| {
                cell.set_width(cell.available_width());
                cell.add_space(3.0);
                slot_row(cell, scheme, data, result, unfolded);
                cell.add_space(3.0);
            });
            if k % 2 == 1 {
                list.painter().set(stripe, egui::Shape::rect_filled(row.response.rect, 0.0, color(scheme.bar)));
            }
        }
    });
}

/// The body of the fold.
fn body(ui: &mut Ui, scheme: &Scheme, analysis: &Analysis, entries: &[&Map], unfolded: &mut HashSet<String>) {
    let sub = color(scheme.sub);
    small(ui, &t("s_fav_text"), sub);
    if entries.is_empty() {
        ui.add_space(GAP);
        small(ui, &t("s_fav_leer"), sub);
        return;
    }
    for archetype in Archetype::ALL {
        archetype_section(ui, scheme, analysis, archetype, unfolded);
    }
    if !analysis.orphan_snubs.is_empty() {
        ui.add_space(GAP);
        ui.label(RichText::new(t("s_fav_snubs_titel")).strong());
        for (snub, mothers) in &analysis.orphan_snubs {
            let line =
                tf("s_fav_snub", &[("snub", Arg::from(snub.as_str())), ("mutter", Arg::from(mothers.join(" / ")))]);
            small(ui, &line, color(scheme.gold));
        }
    }
    if !analysis.included.is_empty() {
        ui.add_space(GAP);
        ui.label(RichText::new(t("s_fav_inkl_titel")).strong());
        for vessel in &analysis.included {
            let parents = vessel.parents.join(", ");
            let line = tf(
                "s_fav_inkl_zeile",
                &[("name", Arg::from(vessel.name.as_str())), ("eltern", Arg::from(parents.as_str()))],
            );
            let shown =
                ui.add(egui::Label::new(RichText::new(line).small().color(sub)).wrap().sense(egui::Sense::hover()));
            shown.on_hover_text(tf("s_fav_inkl_hinweis", &[("eltern", Arg::from(parents.as_str()))]));
        }
    }
    ui.add_space(GAP);
    ui.label(RichText::new(t("s_fa_schmelz_titel")).strong());
    let m = analysis.melt;
    let counts = tf(
        "s_fav_schmelz",
        &[
            ("beides", Arg::from(m.both)),
            ("nur_s", Arg::from(m.melt_only)),
            ("nur_c", Arg::from(m.upgrade)),
            ("keins", Arg::from(m.locked)),
            ("offen", Arg::from(m.unknown)),
        ],
    );
    ui.add(egui::Label::new(counts).wrap());
    small(ui, &t("s_fav_schmelz_hilfe"), sub);
    if !analysis.unknown.is_empty() {
        ui.add_space(GAP);
        small(ui, &tf("s_fav_unbekannt", &[("namen", Arg::from(analysis.unknown.join(", ")))]), sub);
    }
    ui.add_space(GAP);
    small(ui, &t("s_fav_quelle"), sub);
    let source = &fleet_analysis::data().source;
    let date = shown_date(&source.ships_updated, language::current());
    small(ui, &tf("s_fav_stand", &[("datum", Arg::from(date))]), sub);
    if widgets::external_link(ui, scheme, &t("s_fav_link")).clicked() && !source.repository.is_empty() {
        browser::open_in_browser(&source.repository);
    }
}

/// The fleet analysis over the ship list: closed at first (the button "Fleet analysis" in the head card of the page
/// opens it), built only while open.
pub fn show(ui: &mut Ui, scheme: &Scheme, view: &mut FleetView, entries: &[&Map]) {
    if !view.open {
        return;
    }
    let analysis = view.analysis(entries);
    body(ui, scheme, &analysis, entries, &mut view.unfolded);
}

#[cfg(test)]
mod tests {
    use super::*;
    use versekit_core::jsonio::{self, Value};
    use versekit_core::language::{Lang, with_language};

    const HANGAR: &str = r#"[
        {"name": "Carrack", "herkunft": "pledge", "schmelzbar": true},
        {"name": "Cutlass Black", "herkunft": "pledge", "schmelzbar": false},
        {"name": "Prospector", "herkunft": "pledge"},
        {"name": "Fury", "herkunft": "pledge"},
        {"name": "Mystery Boat", "herkunft": "ingame"}]"#;

    fn entries(text: &str) -> Vec<Map> {
        match jsonio::loads(text).expect("JSON") {
            Value::Array(items) => items.into_iter().filter_map(|v| v.as_object().cloned()).collect(),
            _ => panic!("array"),
        }
    }

    /// One frame of the fold in a page `width` wide; the texts of all nodes
    /// and how far the content reaches over the right edge.
    fn frame(ctx: &egui::Context, view: &mut FleetView, list: &[Map], width: f32) -> (Vec<String>, f32) {
        let scheme = versekit_core::theme::current();
        let refs: Vec<&Map> = list.iter().collect();
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, 6000.0))),
            ..Default::default()
        };
        let mut over = 0.0;
        let mut output = ctx.run_ui(input, |ui| {
            show(ui, scheme, view, &refs);
            over = (ui.min_rect().right() - ui.ctx().content_rect().right()).max(0.0);
        });
        output.textures_delta.clear();
        // A label shows up as a node and again as its text: each text once.
        let mut texts: Vec<String> = output
            .platform_output
            .accesskit_update
            .map(|u| u.nodes.iter().filter_map(|(_, n)| n.label().or(n.value()).map(str::to_owned)).collect())
            .unwrap_or_default();
        texts.sort();
        texts.dedup();
        (texts, over)
    }

    fn context() -> egui::Context {
        let ctx = egui::Context::default();
        crate::install(&ctx);
        ctx.enable_accesskit();
        ctx
    }

    fn settled(ctx: &egui::Context, view: &mut FleetView, list: &[Map], width: f32) -> (Vec<String>, f32) {
        let mut last = frame(ctx, view, list, width);
        for _ in 0..3 {
            last = frame(ctx, view, list, width);
        }
        last
    }

    #[test]
    fn the_data_date_reads_in_the_language() {
        assert_eq!(shown_date("2026-10-19", Lang::De), "19.10.2026");
        assert_eq!(shown_date("2026-10-09", Lang::En), "9 Oct 2026");
        assert_eq!(shown_date("garbled", Lang::De), "garbled");
        assert_eq!(shown_date("2026-13-01", Lang::En), "2026-13-01");
    }

    #[test]
    fn the_gap_badge_stands_at_the_same_edge_whatever_the_name() {
        // The Cutlass Black is only a secondary fighter and a secondary
        // transporter: two gaps with names of different length.
        let _language = crate::language_lock();
        let ctx = context();
        let list = entries(r#"[{"name": "Cutlass Black", "herkunft": "pledge"}]"#);
        let mut view = FleetView::with_open(true);
        let gap = t("s_fav_luecke");
        let mut edges: Vec<f64> = Vec::new();
        for _ in 0..4 {
            let refs: Vec<&Map> = list.iter().collect();
            let scheme = versekit_core::theme::current();
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(900.0, 6000.0))),
                ..Default::default()
            };
            let mut output = ctx.run_ui(input, |ui| show(ui, scheme, &mut view, &refs));
            output.textures_delta.clear();
            edges = output
                .platform_output
                .accesskit_update
                .map(|u| {
                    u.nodes
                        .iter()
                        .filter(|(_, n)| n.value() == Some(gap.as_str()) || n.label() == Some(gap.as_str()))
                        .filter_map(|(_, n)| n.bounds().map(|b| b.x1))
                        .collect()
                })
                .unwrap_or_default();
        }
        assert!(edges.len() >= 2, "{edges:?}");
        assert!(edges.iter().all(|x| (x - edges[0]).abs() < 0.5), "the badge moves with the name: {edges:?}");
    }

    #[test]
    fn closed_it_shows_nothing_and_builds_nothing() {
        let _language = crate::language_lock();
        let ctx = context();
        let list = entries(HANGAR);
        let mut view = FleetView::default();
        let (texts, _) = settled(&ctx, &mut view, &list, 900.0);
        assert!(texts.is_empty(), "the button that opens it stands in the head of the page: {texts:?}");
        assert!(view.cache.is_none(), "nothing is analysed while closed");
    }

    #[test]
    fn open_it_shows_the_groups_the_one_not_covered_line_and_the_source() {
        let _language = crate::language_lock();
        let ctx = context();
        let list = entries(HANGAR);
        let mut view = FleetView { open: true, ..FleetView::default() };
        let (texts, _) = settled(&ctx, &mut view, &list, 900.0);
        for key in [
            "s_fav_gruppe_kampf",
            "s_fav_gruppe_versorgung",
            "s_fav_gruppe_reisen",
            "s_fav_quelle",
            "s_fav_schmelz_hilfe",
        ] {
            assert!(texts.contains(&t(key)), "{key}: {texts:?}");
        }
        assert!(!texts.iter().any(|x| x.starts_with("s_fa")), "a text key without a text: {texts:?}");
        let covered = t("s_fav_nicht_abgedeckt");
        assert_eq!(
            texts.iter().filter(|x| x.starts_with(&covered)).count(),
            3,
            "one line per group, not one per loop: {texts:?}"
        );
        assert!(texts.iter().any(|x| x.contains("Prospector")), "{texts:?}");
        // The orphan snub, the vehicles that come along, the unknown name.
        assert!(texts.iter().any(|x| x.contains("Fury") && x.contains("Starlancer TAC")), "{texts:?}");
        assert!(texts.iter().any(|x| x.contains("C8 Pisces") && x.contains("Carrack Expedition w/C8X")), "{texts:?}");
        assert!(texts.iter().any(|x| x.contains("Mystery Boat")), "{texts:?}");
    }

    #[test]
    fn secondary_roles_stay_folded_until_unfolded() {
        let _language = crate::language_lock();
        let ctx = context();
        let list = entries(r#"[{"name": "Cutlass Black", "herkunft": "pledge"}]"#);
        let mut view = FleetView { open: true, ..FleetView::default() };
        let (texts, _) = settled(&ctx, &mut view, &list, 900.0);
        let head = tf("s_fav_nebenrollen", &[("n", Arg::from(1usize))]);
        assert!(texts.contains(&head), "{texts:?}");
        // The ship stands as a primary cargo ship; as a fighter it is only
        // in the fold, as the bare name.
        assert!(!texts.iter().any(|x| x == "Cutlass Black"), "{texts:?}");
        view.unfolded.insert("fighter".into());
        let (texts, _) = settled(&ctx, &mut view, &list, 900.0);
        assert!(texts.iter().any(|x| x == "Cutlass Black"), "{texts:?}");
    }

    #[test]
    fn the_analysis_is_made_again_only_when_the_hangar_changes() {
        let mut view = FleetView::default();
        let mut list = entries(HANGAR);
        let refs: Vec<&Map> = list.iter().collect();
        let first = view.analysis(&refs);
        assert!(Arc::ptr_eq(&first, &view.analysis(&refs)));
        // What the analysis does not read does not remake it.
        let mut ready = list.clone();
        versekit_core::jsonio::map_set(&mut ready[0], "einsatzbereit", Value::Bool(true));
        let refs_ready: Vec<&Map> = ready.iter().collect();
        assert!(Arc::ptr_eq(&first, &view.analysis(&refs_ready)));
        // A melt choice does.
        versekit_core::jsonio::map_set(&mut list[0], "schmelz_wahl", Value::str("nein"));
        let refs_changed: Vec<&Map> = list.iter().collect();
        let second = view.analysis(&refs_changed);
        assert!(!Arc::ptr_eq(&first, &second));
        assert_eq!(second.melt.upgrade, first.melt.upgrade + 1);
        assert_eq!(second.melt.both, first.melt.both - 1);
        // So do the name, the maker and the origin.
        for (key, value) in [("name", "Hammerhead"), ("hersteller", "Aegis"), ("herkunft", "ingame")] {
            let mut changed = list.clone();
            versekit_core::jsonio::map_set(&mut changed[2], key, Value::str(value));
            let refs: Vec<&Map> = changed.iter().collect();
            assert!(!Arc::ptr_eq(&second, &view.analysis(&refs)), "{key}");
            view.analysis(&refs_changed);
        }
    }

    #[test]
    fn an_empty_hangar_says_what_to_do() {
        let _language = crate::language_lock();
        let ctx = context();
        let mut view = FleetView { open: true, ..FleetView::default() };
        let (texts, _) = settled(&ctx, &mut view, &[], 900.0);
        assert!(texts.contains(&t("s_fav_leer")), "{texts:?}");
        assert!(!texts.contains(&t("s_fav_gruppe_kampf")), "{texts:?}");
    }

    #[test]
    fn open_it_sticks_out_nowhere_in_any_scheme_size_and_language() {
        let list = entries(HANGAR);
        let mut findings = Vec::new();
        for scheme in versekit_core::theme::schemes() {
            for lang in Lang::ALL {
                for points in versekit_core::font_size::POINTS_RANGE.0..=versekit_core::font_size::POINTS_RANGE.1 {
                    for width in [380.0, 880.0] {
                        let ctx = context();
                        crate::apply_size(&ctx, points);
                        let mut view = FleetView { open: true, ..FleetView::default() };
                        // all secondary roles unfolded as well
                        let refs: Vec<&Map> = list.iter().collect();
                        let all = view.analysis(&refs);
                        for r in &all.slots {
                            view.unfolded.insert(r.slot.key(fleet_analysis::data()));
                        }
                        let over = with_language(lang, || {
                            let mut over = 0.0_f32;
                            for _ in 0..4 {
                                let input = egui::RawInput {
                                    screen_rect: Some(egui::Rect::from_min_size(
                                        egui::Pos2::ZERO,
                                        egui::vec2(width, 6000.0),
                                    )),
                                    ..Default::default()
                                };
                                let mut output = ctx.run_ui(input, |ui| {
                                    show(ui, scheme, &mut view, &refs);
                                    over = (ui.min_rect().right() - ui.ctx().content_rect().right()).max(0.0);
                                });
                                output.textures_delta.clear();
                            }
                            over
                        });
                        if over > 0.5 {
                            findings.push(format!(
                                "{} {lang:?} size {points} width {width}: {over:.0} too wide",
                                scheme.id
                            ));
                        }
                    }
                }
            }
        }
        assert!(findings.is_empty(), "{}", findings.join("\n"));
    }
}
