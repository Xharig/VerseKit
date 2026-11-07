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

//! The blueprint progress: how far one is, per area, each area folding out
//! to its kinds; for all blueprints or only the watch list; and which
//! contracts give the most missing blueprints. A click on a kind or a
//! contract opens the blueprint list filtered to it.

use std::collections::{BTreeMap, HashSet};

use egui::{Align, Layout, RichText, Sense, Ui};
use versekit_core::blueprints as bp;
use versekit_core::collection::{self, Collection, NoCatalog, Shrinkage};
use versekit_core::export::{self, FileExports};
use versekit_core::formats;
use versekit_core::language::{self, Arg, t, tf};
use versekit_core::paths::Paths;
use versekit_core::theme::{IconTint, Scheme};

use crate::blueprint_list::{ListData, ListJump};
use crate::layout::GAP;
use crate::widgets::{self, ButtonKind};
use crate::{color, icons};

/// The setting that keeps the watch-list-only choice over a restart.
pub const WATCHLIST_SETTING: &str = "fortschritt_merkliste";
/// How many contracts the list of the most rewarding contracts shows.
const BEST_SHOWN: usize = 10;
/// Width of the kind name in a kind row.
const KIND_NAME_WIDTH: f32 = 170.0;

/// What the progress page is set to; areas and the contract list start folded.
#[derive(Default)]
pub struct ProgressState {
    /// Only the blueprints clicked onto the watch list.
    pub watch_only: bool,
    open_areas: HashSet<&'static str>,
    best_open: bool,
    /// Fewer blueprints than ever before, with the time it was looked up (the collection is read only every
    /// [`SHRINK_LOOK_SECONDS`]).
    shrink: Option<(f64, Option<Shrinkage>)>,
}

/// How often the warning about fewer blueprints reads the collection again, in seconds.
const SHRINK_LOOK_SECONDS: f64 = 5.0;

/// Fewer blueprints than ever before, without moving the high-water mark (looking must not make the warning go).
fn shrinkage_of(paths: &Paths) -> Option<Shrinkage> {
    let exports = FileExports { paths, version: env!("CARGO_PKG_VERSION"), clock: export::now_unix };
    let collection = Collection {
        paths,
        catalog: &NoCatalog,
        names: &crate::blueprint_list::NoEnglishNames,
        exports: &exports,
        clock: collection::now_text,
    };
    collection.shrinkage_state()
}

/// The warning above the progress: how many there are, how many there were, the folder of back then and the one
/// read now.
fn shrink_box(ui: &mut Ui, scheme: &Scheme, shrink: &Shrinkage, now_folder: &str) {
    let earlier = shrink.folder.as_str().unwrap_or_default().to_string();
    if !earlier.is_empty() {
        versekit_core::streamer::learn_path(&earlier);
    }
    egui::Frame::new()
        .fill(color(scheme.surface))
        .stroke(egui::Stroke::new(1.0, color(scheme.gold)))
        .corner_radius(crate::control_radius(scheme))
        .inner_margin(egui::Margin::same(12))
        .show(ui, |card| {
            card.set_width(card.available_width());
            card.label(RichText::new(t("s_schwund_titel")).strong().color(color(scheme.gold)));
            let numbers = [("0", Arg::from(shrink.now)), ("1", Arg::from(shrink.highest.to_string()))];
            card.add(egui::Label::new(RichText::new(tf("s_schwund_text_2", &numbers)).small()).wrap());
            for (caption, place) in [("s_schwund_wo", earlier.as_str()), ("s_schwund_jetzt", now_folder)] {
                if place.is_empty() {
                    continue;
                }
                card.add_space(4.0);
                card.label(RichText::new(t(caption)).small().color(color(scheme.sub)));
                card.add(egui::Label::new(RichText::new(place).small().color(color(scheme.accent))).wrap());
            }
            card.add_space(6.0);
            card.add(egui::Label::new(RichText::new(t("s_schwund_tipp")).small().color(color(scheme.sub))).wrap());
        });
    ui.add_space(GAP);
}

/// Per area and kind: (total, have), for all blueprints or only `only`.
pub fn counts(data: &ListData, only: Option<&HashSet<String>>) -> BTreeMap<usize, BTreeMap<String, (usize, usize)>> {
    let mut areas: BTreeMap<usize, BTreeMap<String, (usize, usize)>> = BTreeMap::new();
    for blueprint in &data.catalog.blueprints {
        if only.is_some_and(|keys| !keys.contains(&blueprint.key)) {
            continue;
        }
        let raw = bp::kind_id(&blueprint.kind);
        let area = bp::TOP_GROUPS.iter().position(|g| *g == bp::top_group(raw)).unwrap_or(bp::TOP_GROUPS.len() - 1);
        let kind = if raw.is_empty() { "—".to_string() } else { bp::kind_readable(raw) };
        let entry = areas.entry(area).or_default().entry(kind).or_default();
        entry.0 += 1;
        if data.have.contains(&blueprint.key) {
            entry.1 += 1;
        }
    }
    areas
}

/// A row that is a click as a whole: hand cursor, returns the click.
fn clickable(ui: &mut Ui, id: impl egui::AsIdSalt, rect: egui::Rect) -> bool {
    ui.interact(rect, ui.scope_id().with(id), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
}

/// The head of a folding part: arrow, title, a number. Returns a click on it.
fn fold_head(ui: &mut Ui, scheme: &Scheme, id: &str, open: bool, title: &str, strong: bool, number: &str) -> bool {
    let row = ui.horizontal(|head| {
        let arrow = if open { "chevron-down" } else { "chevron-right" };
        if let Some(image) = icons::themed(arrow, scheme, IconTint::Normal, 12.0) {
            head.add(image);
        }
        let title = RichText::new(title);
        head.label(if strong { title.strong() } else { title });
        head.label(RichText::new(number).color(color(scheme.sub)));
        head.allocate_space(egui::vec2(head.available_width(), 0.0));
    });
    clickable(ui, id, row.response.rect)
}

/// [`fold_head`] as the first row of a card: arrow, the title in the look of a card title, a number. Returns a
/// click on it.
fn fold_title(ui: &mut Ui, scheme: &Scheme, id: &str, open: bool, title: &str, number: &str) -> bool {
    let right = ui.max_rect().right();
    let row = ui.horizontal(|head| {
        let arrow = if open { "chevron-down" } else { "chevron-right" };
        if let Some(image) = icons::themed(arrow, scheme, IconTint::Normal, 12.0) {
            head.add(image);
        }
        widgets::card_title(head, scheme, title);
        head.label(RichText::new(number).color(color(scheme.sub)));
    });
    // The whole width of the card is the head to click, without taking room in the row.
    let mut rect = row.response.rect;
    rect.max.x = rect.max.x.max(right);
    clickable(ui, id, rect)
}

/// Draws the page; returns where a click wants to go in the blueprint list.
pub fn show(ui: &mut Ui, scheme: &Scheme, data: &ListData, state: &mut ProgressState) -> Option<(ListJump, String)> {
    let mut jump = None;
    ui.label(RichText::new(t("s_fo_lead")).small().color(color(scheme.sub)));
    ui.add_space(4.0);
    // The whole: the warning, the choice of all or the watch list, the big number with its bar. Nothing to count
    // ends the page here.
    let areas = widgets::card(ui, scheme, |ui| {
        widgets::card_title(ui, scheme, &t("s_fo_gesamt_kopf"));
        // Fewer blueprints than ever before: above the numbers one stumbles over.
        if let Some(paths) = data.paths() {
            let now = ui.input(|i| i.time);
            if state.shrink.as_ref().is_none_or(|(at, _)| now - at >= SHRINK_LOOK_SECONDS || now < *at) {
                state.shrink = Some((now, shrinkage_of(paths)));
            }
            if let Some((_, Some(shrink))) = &state.shrink {
                shrink_box(ui, scheme, shrink, &paths.app_folder());
            }
        }
        ui.horizontal_wrapped(|row| {
            for (watch, key) in [(false, "s_fo_alle"), (true, "s_fo_merk")] {
                let kind = if state.watch_only == watch { ButtonKind::Strong } else { ButtonKind::Normal };
                let word = t(key);
                if row.add(widgets::Button::new(&word, kind, scheme)).clicked() {
                    state.watch_only = watch;
                }
            }
        });
        ui.add_space(GAP);

        // Only blueprints clicked onto the watch list: a pattern stands for no
        // particular piece and has no progress.
        let only = state.watch_only.then_some(&data.watched);
        if state.watch_only && !data.catalog.blueprints.iter().any(|b| data.watched.contains(&b.key)) {
            ui.label(RichText::new(t("s_fo_merk_leer")).color(color(scheme.sub)));
            return None;
        }
        let areas = counts(data, only);
        let total: usize = areas.values().flat_map(|kinds| kinds.values()).map(|(n, _)| n).sum();
        let have: usize = areas.values().flat_map(|kinds| kinds.values()).map(|(_, h)| h).sum();
        let share = have as f64 / total.max(1) as f64;
        ui.horizontal(|head| {
            head.label(RichText::new(have.to_string()).heading().strong().color(color(scheme.accent)));
            let of = tf("s_fo_von", &[("0", Arg::from(total)), ("1", Arg::from(100.0 * share))]);
            head.label(RichText::new(of.trim()).color(color(scheme.sub)));
        });
        let width = ui.available_width();
        widgets::progress(ui, scheme, share as f32, width);
        Some(areas)
    })
    .inner?;
    ui.add_space(GAP);

    // The areas, each folding out to its kinds.
    if !areas.is_empty() {
        widgets::card(ui, scheme, |ui| {
            widgets::card_title(ui, scheme, &t("s_fo_bereiche_kopf"));
            let width = ui.available_width();
            for (index, (area, kinds)) in areas.iter().enumerate() {
                if index > 0 {
                    ui.add_space(GAP);
                }
                let key = bp::TOP_GROUPS[(*area).min(bp::TOP_GROUPS.len() - 1)];
                let area_total: usize = kinds.values().map(|(n, _)| n).sum();
                let area_have: usize = kinds.values().map(|(_, h)| h).sum();
                let open = state.open_areas.contains(key);
                let title = t(&format!("gruppe_{key}"));
                if fold_head(ui, scheme, key, open, &title, true, &format!("{area_have} / {area_total}")) {
                    if open {
                        state.open_areas.remove(key);
                    } else {
                        state.open_areas.insert(key);
                    }
                }
                widgets::progress(ui, scheme, area_have as f32 / area_total.max(1) as f32, width);
                if open {
                    ui.indent(("fortschritt-bereich", key), |inner| {
                        let mut rows: Vec<(&String, &(usize, usize))> = kinds.iter().collect();
                        rows.sort_by(|a, b| b.1.0.cmp(&a.1.0).then_with(|| a.0.cmp(b.0)));
                        for (k, (kind, (kind_total, kind_have))) in rows.into_iter().enumerate() {
                            // The stripe goes under the row: its place is taken before the row is drawn.
                            let stripe = inner.painter().add(egui::Shape::Noop);
                            let row = inner.horizontal(|line| {
                                line.add_sized(
                                    [KIND_NAME_WIDTH, line.spacing().interact_size.y],
                                    egui::Label::new(RichText::new(kind).color(color(scheme.sub))).truncate(),
                                );
                                line.with_layout(Layout::right_to_left(Align::Center), |right| {
                                    let count = format!("{kind_have} / {kind_total}");
                                    right.label(RichText::new(count).color(color(scheme.sub)));
                                    let bar = right.available_width().min(260.0);
                                    let part = *kind_have as f32 / (*kind_total).max(1) as f32;
                                    widgets::progress(right, scheme, part, bar);
                                });
                            });
                            let rect = row.response.rect;
                            if k % 2 == 1 {
                                inner.painter().set(stripe, egui::Shape::rect_filled(rect, 0.0, color(scheme.bar)));
                            }
                            // A count like 38 / 70 does not say which ones are missing: the click shows them.
                            if clickable(inner, ("fortschritt-art", kind), rect) {
                                let back = format!("{}: {kind}", t("hf_fortschritt"));
                                jump = Some((ListJump::Kind(kind.clone()), back));
                            }
                        }
                    });
                }
            }
        });
        ui.add_space(GAP);
    }

    // The most rewarding contracts only for all blueprints: it counts missing
    // blueprints over the whole catalog.
    if !state.watch_only {
        let best = bp::worthwhile_contracts(&data.catalog, &data.have);
        let shown = best.len().min(BEST_SHOWN);
        let number = if best.is_empty() { String::new() } else { shown.to_string() };
        widgets::card(ui, scheme, |ui| {
            if fold_title(ui, scheme, "fortschritt-lohnt", state.best_open, &t("s_fo_lohnt"), &number) {
                state.best_open = !state.best_open;
            }
            if state.best_open {
                ui.label(RichText::new(t("s_fo_lohnt_hilfe")).small().color(color(scheme.sub)));
                if best.is_empty() {
                    ui.label(RichText::new(t("s_fo_lohnt_leer")).color(color(scheme.sub)));
                } else if let Some(to) = best_contracts(ui, scheme, &best[..shown]) {
                    let keys = versekit_core::mission_log::literal_blueprints(&data.catalog, &to);
                    jump = Some((ListJump::Contract(to.clone(), keys), format!("{}: {to}", t("s_fo_lohnt"))));
                }
            }
        });
    }
    jump
}

/// The contracts with the most missing blueprints, striped; a click on one
/// returns its title.
fn best_contracts(ui: &mut Ui, scheme: &Scheme, best: &[bp::Worthwhile]) -> Option<String> {
    let lang = language::current();
    let mut picked = None;
    widgets::list_frame(ui, scheme, |list| {
        list.set_width(list.available_width());
        for (k, contract) in best.iter().enumerate() {
            let stripe = list.painter().add(egui::Shape::Noop);
            let row = list.horizontal(|line| {
                line.add_sized(
                    [32.0, line.spacing().interact_size.y],
                    egui::Label::new(RichText::new(contract.missing.to_string()).strong().color(color(scheme.accent))),
                );
                line.vertical(|text| {
                    text.add(egui::Label::new(&contract.title).truncate());
                    let mut parts: Vec<String> = Vec::new();
                    if !contract.faction.is_empty() {
                        parts.push(contract.faction.clone());
                    }
                    if contract.reward > 0 {
                        parts.push(formats::money(lang, contract.reward as f64));
                    }
                    parts.extend(contract.rank.clone());
                    parts.push(tf("s_fo_lohnt_topf", &[("0", Arg::from(contract.missing))]));
                    text.add(
                        egui::Label::new(RichText::new(parts.join(" · ")).small().color(color(scheme.sub))).truncate(),
                    );
                    if let Some(place) = &contract.place {
                        let place = bp::place_text(place);
                        text.add(egui::Label::new(RichText::new(place).small().color(color(scheme.sub))).truncate());
                    }
                    text.add(
                        egui::Label::new(RichText::new(t("s_fo_lohnt_klick")).small().color(color(scheme.sub))).wrap(),
                    );
                });
                // The rest of the row belongs to the click; where the text took it all there is no rest, and a
                // space of no width would still push the row a gap beyond its frame.
                let rest = line.available_width();
                if rest > 0.0 {
                    line.allocate_space(egui::vec2(rest, 0.0));
                }
            });
            let rect = row.response.rect;
            if k % 2 == 1 {
                list.painter().set(stripe, egui::Shape::rect_filled(rect, 0.0, color(scheme.bar)));
            }
            if clickable(list, ("fortschritt-auftrag", k), rect) {
                picked = Some(contract.title.clone());
            }
        }
    });
    picked
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Everything folded out still fits a narrow page, at every size.
    #[test]
    fn folded_out_nothing_sticks_out() {
        let data = ListData::sample();
        let scheme = versekit_core::theme::current();
        let mut findings = Vec::new();
        for points in versekit_core::font_size::POINTS_RANGE.0..=versekit_core::font_size::POINTS_RANGE.1 {
            for width in [420.0, 520.0, 900.0] {
                let ctx = egui::Context::default();
                crate::install(&ctx);
                crate::apply_size(&ctx, points);
                let mut state = ProgressState {
                    open_areas: bp::TOP_GROUPS.iter().copied().collect(),
                    best_open: true,
                    ..ProgressState::default()
                };
                let mut over = 0.0;
                let mut beyond = Vec::new();
                for _ in 0..3 {
                    let input = egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, 2000.0))),
                        ..Default::default()
                    };
                    let mut right = 0.0;
                    let mut output = ctx.run_ui(input, |ui| {
                        show(ui, scheme, &data, &mut state);
                        // The outer area grows with what is drawn: measure against the window.
                        right = ui.ctx().content_rect().right();
                        over = (ui.min_rect().right() - right).max(0.0);
                    });
                    output.textures_delta.clear();
                    // What is drawn beyond the window, to name it in the finding.
                    beyond = output
                        .shapes
                        .iter()
                        .filter(|clipped| clipped.shape.visual_bounding_rect().right() > right + 0.5)
                        .map(|clipped| match &clipped.shape {
                            egui::Shape::Text(text) => format!("Text {:?}", text.galley.text()),
                            other => format!("{:?}", other.visual_bounding_rect()),
                        })
                        .collect();
                }
                if over > 0.5 {
                    findings.push(format!("Größe {points} Breite {width}: {over:.0} zu breit: {beyond:?}"));
                }
            }
        }
        assert!(findings.is_empty(), "{}", findings.join("\n"));
    }

    /// The texts of a few frames of the page at `width`, and how far it sticks out.
    fn drawn(data: &ListData, width: f32) -> (Vec<String>, f32) {
        let scheme = versekit_core::theme::by_id("original").expect("original");
        let ctx = egui::Context::default();
        crate::install(&ctx);
        let mut state = ProgressState::default();
        let (mut texts, mut over) = (Vec::new(), 0.0);
        for _ in 0..3 {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, 2000.0))),
                ..Default::default()
            };
            let mut output = ctx.run_ui(input, |ui| {
                show(ui, scheme, data, &mut state);
                over = (ui.min_rect().right() - ui.ctx().content_rect().right()).max(0.0);
            });
            output.textures_delta.clear();
            texts.clear();
            fn collect(shape: &egui::Shape, into: &mut Vec<String>) {
                match shape {
                    egui::Shape::Text(text) => into.push(text.galley.text().to_string()),
                    egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| collect(s, into)),
                    _ => {}
                }
            }
            output.shapes.iter().for_each(|clipped| collect(&clipped.shape, &mut texts));
        }
        (texts, over)
    }

    #[test]
    fn fewer_blueprints_than_ever_before_stand_above_the_numbers_with_both_folders() {
        use versekit_core::paths::Host;
        use versekit_core::pypath::Flavor;
        let _language = crate::language_lock();
        language::set_language(language::Choice::De);
        let home = std::env::temp_dir().join(format!("versekit-schwund-{}", versekit_core::testdir::run_tag()));
        let home = home.to_string_lossy().replace('\\', "/");
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).unwrap();
        let paths = Paths::new(Host::new(
            Flavor::Posix,
            &[("SC_BP_HOME", home.as_str()), ("HOME", home.as_str()), ("XDG_CONFIG_HOME", home.as_str())],
            &home,
        ));
        let exports = FileExports { paths: &paths, version: "test", clock: export::now_unix };
        let collection = Collection {
            paths: &paths,
            catalog: &NoCatalog,
            names: &crate::blueprint_list::NoEnglishNames,
            exports: &exports,
            clock: collection::now_text,
        };
        let mut stock = collection.load();
        collection.add(&mut stock, "Arclight Pistol", "log", None);
        collection.add(&mut stock, "Pembroke Helmet", "log", None);
        assert!(collection.save(&mut stock));
        let mark = collection.high_water_file();
        assert!(mark.starts_with(&home), "the mark lies in the test folder: {mark}");
        let data = ListData::sample().with_paths(Paths::new(paths.host.clone()));

        // As many as ever: no warning.
        assert!(collection.check_shrinkage(&stock).is_none());
        let (texts, _) = drawn(&data, 900.0);
        assert!(!texts.contains(&t("s_schwund_titel")), "{texts:?}");

        // The mark of a folder with more: the warning, both folders, and looking does not move the mark.
        let earlier = format!("{home}/frueher");
        std::fs::write(&mark, format!("{{\"bauplaene\": 5, \"ordner\": \"{earlier}\"}}")).unwrap();
        let numbers = [("0", Arg::from(2usize)), ("1", Arg::from("5"))];
        for width in [420.0, 900.0] {
            let (texts, over) = drawn(&data, width);
            assert!(texts.contains(&t("s_schwund_titel")), "{texts:?}");
            assert!(texts.contains(&tf("s_schwund_text_2", &numbers)), "{texts:?}");
            assert!(texts.contains(&earlier) && texts.contains(&paths.app_folder()), "{texts:?}");
            assert!(over <= 0.5, "width {width}: {over} too wide");
        }
        assert!(std::fs::read_to_string(&mark).unwrap().contains("\"bauplaene\": 5"), "the mark stays");
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn counts_add_up_and_the_watch_list_narrows() {
        let data = ListData::sample();
        let all = counts(&data, None);
        let total: usize = all.values().flat_map(|k| k.values()).map(|(n, _)| n).sum();
        assert_eq!(total, data.catalog.blueprints.len());
        let areas: Vec<usize> = all.keys().copied().collect();
        assert_eq!(areas, [0, 1, 2], "ship, hand, worn — in this order");
        let watched = counts(&data, Some(&data.watched));
        let watched_total: usize = watched.values().flat_map(|k| k.values()).map(|(n, _)| n).sum();
        assert_eq!(watched_total, data.watched.len());
    }
}
