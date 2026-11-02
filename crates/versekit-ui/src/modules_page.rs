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

//! The page "Module" (`modules_page` of 4.12): a switch for each module that hides its pages in the menu.
//! The rules are in `versekit_core::modules`; the window asks it which pages are hidden.

use egui::{RichText, Ui, WidgetInfo};
use versekit_core::language::{Arg, t, tf};
use versekit_core::modules;
use versekit_core::paths::Paths;
use versekit_core::theme::Scheme;

use crate::settings::Said;
use crate::{color, settings, widgets};

/// What the page keeps.
#[derive(Default)]
pub struct ModulesState {
    /// The line for the foot of the window (a notice).
    notice: Option<Said>,
    /// A switch was flipped: the window asks for the hidden pages again.
    changed: bool,
}

impl ModulesState {
    /// The line for the foot, once.
    pub fn take_notice(&mut self) -> Option<Said> {
        self.notice.take()
    }

    /// Whether a module was switched since the last ask, once.
    pub fn take_changed(&mut self) -> bool {
        std::mem::take(&mut self.changed)
    }
}

/// The names of the pages a module hides, as the menu shows them.
pub(crate) fn pages_of(group: &str) -> String {
    modules::SWITCHABLE
        .iter()
        .chain(modules::ALSO_HIDES.iter())
        .filter(|(name, _)| *name == group)
        .flat_map(|(_, pages)| pages.iter().map(|page| t(page)))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Draws the page; a switch is saved at once.
pub fn show_page(ui: &mut Ui, scheme: &Scheme, state: &mut ModulesState, paths: &Paths) {
    ui.label(RichText::new(t("s_mo_lead")).small().color(color(scheme.sub)));
    ui.add_space(8.0);
    for (group, _) in modules::SWITCHABLE {
        let title = t(modules::label(group));
        let mut on = modules::enabled(paths, group);
        let mut flipped = false;
        widgets::card(ui, scheme, |ui| {
            let hint = tf("s_mo_enthaelt", &[("0", Arg::from(pages_of(group).as_str()))]);
            settings::setting_row(ui, scheme, &title, &hint, false, |row| {
                let switch = row.add(widgets::Toggle::new(&mut on, scheme));
                let (enabled, now) = (switch.enabled(), on);
                switch.widget_info(|| WidgetInfo::selected(egui::Role::CheckBox, enabled, now, &title));
                flipped = switch.changed();
            });
        });
        ui.add_space(8.0);
        if flipped {
            state.notice = Some(if modules::set_enabled(paths, group, on) {
                state.changed = true;
                Said::note(format!("{title}: {}", if on { t("e_an") } else { t("e_aus") }))
            } else {
                Said::fault(t("s_hl_fehler"))
            });
        }
    }
    ui.add_space(4.0);
    ui.label(RichText::new(t("s_mo_hinweis")).small().color(color(scheme.sub)));
}

#[cfg(test)]
mod tests {
    use egui::{Event, Pos2, Rect};
    use versekit_core::language::{self, Choice};
    use versekit_core::paths::Host;
    use versekit_core::pypath::Flavor;
    use versekit_core::theme;

    use super::*;

    fn scratch(name: &str) -> String {
        let dir =
            std::env::temp_dir().join(format!("versekit-modulepage-{}-{name}", versekit_core::testdir::run_tag()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("folder");
        dir.to_string_lossy().replace('\\', "/")
    }

    struct Bench {
        ctx: egui::Context,
        state: ModulesState,
        paths: Paths,
        home: String,
    }

    impl Bench {
        fn new(name: &str) -> Bench {
            let ctx = egui::Context::default();
            crate::install(&ctx);
            ctx.enable_accesskit();
            let home = scratch(name);
            let paths = Paths::new(Host::new(Flavor::Posix, &[("SC_BP_HOME", &home), ("HOME", &home)], &home));
            Bench { ctx, state: ModulesState::default(), paths, home }
        }

        fn frame(&mut self, events: Vec<Event>) -> (Vec<String>, Vec<(egui::Role, String, Rect)>) {
            let scheme = theme::by_id("original").expect("original");
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(1000.0, 1200.0))),
                events,
                ..Default::default()
            };
            let (state, paths) = (&mut self.state, &self.paths);
            let mut output = self.ctx.run_ui(input, |ui| show_page(ui, scheme, state, paths));
            output.textures_delta.clear();
            let mut texts = Vec::new();
            fn collect(shape: &egui::Shape, into: &mut Vec<String>) {
                match shape {
                    egui::Shape::Text(text) => into.push(text.galley.text().to_string()),
                    egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| collect(s, into)),
                    _ => {}
                }
            }
            output.shapes.iter().for_each(|clipped| collect(&clipped.shape, &mut texts));
            let nodes = output
                .platform_output
                .accesskit_update
                .map(|update| {
                    update
                        .nodes
                        .iter()
                        .filter_map(|(_, node)| {
                            let b = node.bounds()?;
                            let rect = Rect::from_min_max(
                                Pos2::new(b.x0 as f32, b.y0 as f32),
                                Pos2::new(b.x1 as f32, b.y1 as f32),
                            );
                            Some((node.role(), node.label().unwrap_or_default().to_string(), rect))
                        })
                        .collect()
                })
                .unwrap_or_default();
            (texts, nodes)
        }

        fn settle(&mut self) -> (Vec<String>, Vec<(egui::Role, String, Rect)>) {
            let mut last = (Vec::new(), Vec::new());
            for _ in 0..4 {
                last = self.frame(Vec::new());
            }
            last
        }

        fn flip(&mut self, name: &str) {
            let nodes = self.settle().1;
            let at = nodes
                .iter()
                .find(|(role, label, _)| *role == egui::Role::CheckBox && label == name)
                .map(|(_, _, rect)| rect.center())
                .unwrap_or_else(|| panic!("no switch `{name}`: {nodes:?}"));
            let button = |pressed| Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            };
            self.frame(vec![Event::PointerMoved(at)]);
            self.frame(vec![button(true)]);
            self.frame(vec![button(false)]);
            self.settle();
        }
    }

    fn setup() -> std::sync::MutexGuard<'static, ()> {
        let guard = crate::language_lock();
        theme::set_current("original");
        language::set_language(Choice::De);
        guard
    }

    #[test]
    fn each_module_has_a_switch_with_its_pages_and_a_flip_is_saved_and_told() {
        let _language = setup();
        let mut bench = Bench::new("schalter");
        let texts = bench.settle().0;
        for (group, _) in modules::SWITCHABLE {
            assert!(texts.contains(&t(modules::label(group))), "{group}: {texts:?}");
        }
        let contains = tf("s_mo_enthaelt", &[("0", Arg::from(pages_of("handel").as_str()))]);
        assert!(texts.contains(&contains), "`{contains}` is not shown: {texts:?}");
        assert!(contains.contains("Shops") && contains.contains("Routen"), "{contains}");
        let basetool = pages_of("basetool");
        assert!(basetool.contains(&t("hf_staffellager")), "the unit storage is named with the Basetool: {basetool}");
        assert!(texts.contains(&t("s_mo_hinweis")));
        let handel = t("hf_gruppe_handel");
        bench.flip(&handel);
        assert!(!modules::enabled(&bench.paths, "handel"), "the switch is in the file at once");
        assert!(bench.state.take_changed(), "the window is told");
        assert_eq!(bench.state.take_notice(), Some(Said::note(format!("{handel}: {}", t("e_aus")))));
        assert!(modules::enabled(&bench.paths, "schiffe"), "the others stay");
        bench.flip(&handel);
        assert!(modules::enabled(&bench.paths, "handel"));
        assert_eq!(bench.state.take_notice(), Some(Said::note(format!("{handel}: {}", t("e_an")))));
        assert!(bench.state.take_changed() && !bench.state.take_changed(), "told once per flip");
        let _ = std::fs::remove_dir_all(&bench.home);
    }
}
