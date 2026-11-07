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

//! The game channels at the foot of the page „Installation & Pfade“ (`pages._channel_states` of 4.12): one
//! bubble per installation that is found, with the state of its `user.cfg`. Real data only.

use std::time::Duration;

use egui::{RichText, Ui};
use versekit_core::language::t;
use versekit_core::paths::Paths;
use versekit_core::theme::Scheme;
use versekit_core::usercfg::{self, Status};

use crate::color;
#[cfg(test)]
use crate::widgets;

/// How long a look at the files holds (the page is only drawn now and then, the files change rarely).
const LOOK_SECONDS: f64 = 2.0;

/// The channels with the state of their `user.cfg`, looked at now and then.
#[derive(Default)]
pub(super) struct ChannelStates {
    /// The time of the last look; `None` before the first.
    looked_at: Option<f64>,
    /// (channel, state), in the order of the channels.
    list: Vec<(String, Status)>,
}

impl ChannelStates {
    fn look(&mut self, paths: &Paths, now: f64) {
        if self.looked_at.is_some_and(|at| now - at < LOOK_SECONDS) {
            return;
        }
        self.looked_at = Some(now);
        self.list = paths
            .installed_channels()
            .into_iter()
            .map(|(name, folder, _)| {
                let status = usercfg::status(&folder);
                (name, status)
            })
            .collect();
    }

    /// The text of the state, and the tone of its bubble: found and complete in the accent, a missing line or
    /// file in gold, no installation in grey.
    fn word(status: Status, scheme: &Scheme) -> (String, versekit_core::theme::Color) {
        match status {
            Status::Ok => (t("s_uc_ok"), scheme.accent),
            Status::NoLanguage => (t("s_uc_ohne_sprache"), scheme.gold),
            Status::NoFile => (t("s_uc_ohne_datei"), scheme.gold),
            Status::NoInstall => (t("s_uc_keine"), scheme.sub),
        }
    }

    /// The inside of the card, below its group title: a line of explanation and a line for each channel in the size
    /// of the rest of the page, one below the other (a row of them would run out of a narrow window). Without
    /// `paths` (the sample) nothing is looked at and the list stays as it is.
    pub(super) fn rows(&mut self, ui: &mut Ui, scheme: &Scheme, paths: Option<&Paths>) {
        if let Some(paths) = paths {
            let now = ui.input(|i| i.time);
            self.look(paths, now);
            ui.ctx().request_repaint_after(Duration::from_secs_f64(LOOK_SECONDS));
        }
        ui.add(egui::Label::new(RichText::new(t("s_uc_kanaele_h")).small().color(color(scheme.sub))).wrap());
        ui.add_space(6.0);
        if self.list.is_empty() {
            ui.add(egui::Label::new(RichText::new(t("s_uc_keine_kanaele")).color(color(scheme.sub))).wrap());
            return;
        }
        for (name, status) in &self.list {
            let (word, tone) = Self::word(*status, scheme);
            ui.add(egui::Label::new(RichText::new(format!("{name}: {word}")).color(color(tone))).wrap());
            ui.add_space(4.0);
        }
    }

    /// The block alone in a card of its own, with its group title.
    #[cfg(test)]
    pub(super) fn show(&mut self, ui: &mut Ui, scheme: &Scheme, paths: &Paths) {
        widgets::card(ui, scheme, |ui| {
            ui.set_min_width(ui.available_width());
            crate::settings::setting_group(ui, scheme, &t("s_uc_kanaele"));
            self.rows(ui, scheme, Some(paths));
        });
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use versekit_core::language;
    use versekit_core::paths::Host;
    use versekit_core::pypath::Flavor;
    use versekit_core::theme;

    use super::*;

    /// A user with the game folder `<root>/StarCitizen/<channel>`; each channel is (name, Data.p4k there, user.cfg).
    fn world(name: &str, channels: &[(&str, bool, Option<&str>)]) -> (String, Paths) {
        let root = std::env::temp_dir()
            .join(format!("versekit-channelstates-{name}-{}", versekit_core::testdir::run_tag()))
            .to_string_lossy()
            .replace('\\', "/");
        let _ = fs::remove_dir_all(&root);
        for (channel, p4k, cfg) in channels {
            let folder = format!("{root}/StarCitizen/{channel}");
            fs::create_dir_all(&folder).unwrap();
            if *p4k {
                fs::write(format!("{folder}/Data.p4k"), "").unwrap();
            }
            if let Some(cfg) = cfg {
                fs::write(format!("{folder}/user.cfg"), cfg).unwrap();
            }
        }
        let first = format!("{root}/StarCitizen/{}", channels[0].0);
        let data = format!("{root}/data");
        fs::create_dir_all(&data).unwrap();
        let vars = [("HOME", root.as_str()), ("SC_BP_HOME", data.as_str()), ("SC_INSTALL_DIR", first.as_str())];
        let paths = Paths::new(Host::new(Flavor::Posix, &vars, &root));
        (root, paths)
    }

    /// The texts a frame draws.
    fn drawn(states: &mut ChannelStates, paths: &Paths, time: f64) -> Vec<String> {
        fn collect(shape: &egui::Shape, into: &mut Vec<String>) {
            match shape {
                egui::Shape::Text(text) => into.push(text.galley.text().to_string()),
                egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| collect(s, into)),
                _ => {}
            }
        }
        let ctx = egui::Context::default();
        crate::install(&ctx);
        let input = egui::RawInput { time: Some(time), ..Default::default() };
        let scheme = theme::by_id("original").expect("original");
        let mut output = ctx.run_ui(input, |ui| states.show(ui, scheme, paths));
        output.textures_delta.clear();
        let mut texts = Vec::new();
        output.shapes.iter().for_each(|clipped| collect(&clipped.shape, &mut texts));
        texts
    }

    #[test]
    fn each_channel_gets_a_bubble_with_the_state_of_its_user_cfg_and_a_look_holds_for_a_while() {
        let _language = crate::language_lock();
        language::set_language(language::Choice::De);
        let (root, paths) = world(
            "blasen",
            &[
                ("LIVE", true, Some("g_language = german_(germany)\n")),
                ("HOTFIX", true, Some("sys_MaxFPS = 60\n")),
                ("PTU", true, None),
                ("EPTU", false, None),
            ],
        );
        let mut states = ChannelStates::default();
        let texts = drawn(&mut states, &paths, 10.0);
        for (channel, key) in
            [("LIVE", "s_uc_ok"), ("HOTFIX", "s_uc_ohne_sprache"), ("PTU", "s_uc_ohne_datei"), ("EPTU", "s_uc_keine")]
        {
            let bubble = format!("{channel}: {}", t(key));
            assert!(texts.contains(&bubble), "{bubble}: {texts:?}");
        }
        // The title stands as written, as the group titles of the cards above.
        let title = t("s_uc_kanaele");
        assert!(texts.contains(&title) && texts.contains(&t("s_uc_kanaele_h")), "{texts:?}");
        // The order of the channels is the one of the game.
        let at = |channel: &str| texts.iter().position(|text| text.starts_with(channel)).unwrap();
        assert!(at("LIVE") < at("HOTFIX") && at("HOTFIX") < at("PTU") && at("PTU") < at("EPTU"));
        // A file changed a moment later is not looked at yet; after the time it is.
        fs::write(format!("{root}/StarCitizen/HOTFIX/user.cfg"), "g_language = english\n").unwrap();
        let texts = drawn(&mut states, &paths, 10.5);
        assert!(texts.contains(&format!("HOTFIX: {}", t("s_uc_ohne_sprache"))), "{texts:?}");
        let texts = drawn(&mut states, &paths, 13.0);
        assert!(texts.contains(&format!("HOTFIX: {}", t("s_uc_ok"))), "{texts:?}");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn without_any_channel_the_block_says_so() {
        let _language = crate::language_lock();
        language::set_language(language::Choice::De);
        let (root, paths) = world("leer", &[("LIVE", true, None)]);
        // The game folder goes: no channel is left.
        fs::remove_dir_all(format!("{root}/StarCitizen")).unwrap();
        let mut states = ChannelStates::default();
        let texts = drawn(&mut states, &paths, 1.0);
        assert!(texts.contains(&t("s_uc_keine_kanaele")), "{texts:?}");
        let _ = fs::remove_dir_all(root);
    }
}
