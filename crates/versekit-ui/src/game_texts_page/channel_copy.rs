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

//! The block "Copy settings to another channel" on the page „Spiel“: source and target channel, the parts to take
//! along (graphics, controls, `user.cfg`) and one button with a question before it writes. The work is
//! `versekit_core::channel_copy`; this block only chooses and tells what happened.

use egui::{RichText, Ui};
use versekit_core::channel_copy::{self, Available, CopyError, Part, Report};
use versekit_core::language::{Arg, t, tf};
use versekit_core::paths::{CHANNELS, Paths};
use versekit_core::theme::Scheme;

use super::{GameTextsState, hint};
use crate::color;
use crate::widgets::{self, ButtonKind};

/// What the block remembers.
pub(super) struct CopyBlock {
    /// The game folders of the channels: where from and where to; empty before the first look.
    pub(super) source: String,
    pub(super) target: String,
    /// The parts to take along.
    pub(super) parts: [bool; 3],
    /// The copy waits for the answer to the question.
    pub(super) asking: bool,
    /// What the source has to give (read when the source is chosen and after a copy).
    have: Option<(String, Available)>,
    /// The last thing said, and whether it was a refusal.
    pub(super) said: Option<(String, bool)>,
}

impl Default for CopyBlock {
    fn default() -> CopyBlock {
        CopyBlock {
            source: String::new(),
            target: String::new(),
            parts: [true; 3],
            asking: false,
            have: None,
            said: None,
        }
    }
}

fn part_name(part: Part) -> String {
    match part {
        Part::Graphics => t("s_kk_grafik"),
        Part::Controls => t("s_kk_steuerung"),
        Part::UserCfg => t("s_uc_titel"),
    }
}

fn part_index(part: Part) -> usize {
    channel_copy::PARTS.iter().position(|p| *p == part).unwrap_or(0)
}

/// The channel name of a game folder (the last part of its path), for the messages.
fn channel_of(channels: &[(String, String)], folder: &str) -> String {
    channels.iter().find(|(_, f)| f == folder).map(|(name, _)| name.clone()).unwrap_or_default()
}

impl CopyBlock {
    fn chosen(&self) -> Vec<Part> {
        channel_copy::PARTS.into_iter().filter(|p| self.parts[part_index(*p)]).collect()
    }

    fn say(&mut self, text: String, bad: bool) {
        self.said = Some((text, bad));
    }

    /// Why the button does nothing now, if it does not.
    fn blocked(&mut self, paths: &Paths, channels: &[(String, String)]) -> Option<String> {
        if channels.len() < 2 {
            return Some(t("s_kk_zwei"));
        }
        if self.source == self.target {
            return Some(t("s_kk_gleich"));
        }
        let chosen = self.chosen();
        if chosen.is_empty() {
            return Some(t("s_kk_nichts_gewaehlt"));
        }
        if self.have.as_ref().is_none_or(|(folder, _)| *folder != self.source) {
            self.have = Some((self.source.clone(), channel_copy::available(paths, &self.source)));
        }
        let have = self.have.as_ref().map(|(_, have)| *have).unwrap_or_default();
        chosen.into_iter().find(|p| !have.has(*p)).map(|missing| {
            tf("s_kk_leer_quelle", &[("0", Arg::from(part_name(missing))), ("1", Arg::from(channel_of(channels, &self.source)))])
        })
    }

    /// The chosen parts as one line (`Grafik, Steuerung`).
    fn parts_line(&self) -> String {
        self.chosen().into_iter().map(part_name).collect::<Vec<_>>().join(", ")
    }

    fn run(&mut self, paths: &Paths, channels: &[(String, String)]) {
        let target_name = channel_of(channels, &self.target);
        let result = channel_copy::copy(paths, &self.source, &self.target, &self.chosen());
        self.have = None;
        match result {
            Ok(report) => {
                let text = done_text(&report, &target_name);
                self.say(text, false);
            }
            Err(error) => {
                let text = error_text(&error, &target_name, &channel_of(channels, &self.source));
                self.say(text, true);
            }
        }
    }
}

/// What a finished copy says: the totals and one line per part with the files.
fn done_text(report: &Report, target: &str) -> String {
    if report.written() == 0 {
        return tf("s_kk_schon_gleich", &[("0", Arg::from(target))]);
    }
    let mut text = tf(
        "s_kk_fertig",
        &[
            ("0", Arg::from(target)),
            ("1", Arg::from(report.written())),
            ("2", Arg::from(report.unchanged())),
            ("3", Arg::from(report.backups())),
        ],
    );
    for part in &report.parts {
        if !part.written.is_empty() {
            text.push_str(&format!("\n{}: {}", part_name(part.part), part.written.join(", ")));
        }
    }
    text
}

fn error_text(error: &CopyError, target: &str, source: &str) -> String {
    match error {
        CopyError::NothingChosen => t("s_kk_nichts_gewaehlt"),
        CopyError::SameFolder => t("s_kk_gleich"),
        CopyError::NothingToGive(part) => {
            tf("s_kk_leer_quelle", &[("0", Arg::from(part_name(*part))), ("1", Arg::from(source))])
        }
        CopyError::GameNotQuiet(reason) => reason.clone(),
        CopyError::Damaged(part, what) => {
            tf("s_kk_beschaedigt", &[("0", Arg::from(part_name(*part))), ("1", Arg::from(what.as_str()))])
        }
        CopyError::Failed { part, reason, done } => tf(
            "s_kk_abgebrochen",
            &[
                ("0", Arg::from(part_name(*part))),
                ("1", Arg::from(target)),
                ("2", Arg::from(reason.as_str())),
                ("3", Arg::from(done.written())),
            ],
        ),
    }
}

/// One row of channel buttons; gives the folder chosen.
fn channel_row(ui: &mut Ui, scheme: &Scheme, label: &str, channels: &[(String, String)], current: &str) -> Option<String> {
    let mut picked = None;
    ui.horizontal_wrapped(|row| {
        row.label(label);
        for name in CHANNELS {
            let Some((_, folder)) = channels.iter().find(|(channel, _)| channel == name) else { continue };
            let kind = if folder == current { ButtonKind::Strong } else { ButtonKind::Normal };
            if row.add(widgets::Button::new(name, kind, scheme)).clicked() && folder != current {
                picked = Some(folder.clone());
            }
        }
    });
    picked
}

pub(super) fn show(ui: &mut Ui, scheme: &Scheme, state: &mut GameTextsState) {
    let Some(paths) = state.paths.clone() else { return };
    crate::settings::setting_group(ui, scheme, &t("s_kk_titel"));
    hint(ui, scheme, &t("s_kk_lead"));
    ui.add_space(4.0);
    // Only installations with a Data.p4k: a folder without it is no game.
    let channels: Vec<(String, String)> =
        paths.installed_channels().into_iter().filter(|(_, _, installed)| *installed).map(|(n, f, _)| (n, f)).collect();
    let block = &mut state.channel_copy;
    if channels.len() < 2 {
        hint(ui, scheme, &t("s_kk_zwei"));
        return;
    }
    // The first channel is the source, the second the target, until the player chooses.
    if !channels.iter().any(|(_, f)| *f == block.source) {
        block.source.clone_from(&channels[0].1);
    }
    if !channels.iter().any(|(_, f)| *f == block.target) {
        block.target.clone_from(&channels[1].1);
    }
    let from = t("s_kk_von");
    let to = t("s_kk_nach");
    if let Some(folder) = channel_row(ui, scheme, &from, &channels, &block.source.clone()) {
        block.source = folder;
        block.asking = false;
        block.said = None;
    }
    if let Some(folder) = channel_row(ui, scheme, &to, &channels, &block.target.clone()) {
        block.target = folder;
        block.asking = false;
        block.said = None;
    }
    ui.add_space(6.0);
    for part in channel_copy::PARTS {
        let help = match part {
            Part::Graphics => t("s_kk_grafik_h"),
            Part::Controls => t("s_kk_steuerung_h"),
            Part::UserCfg => t("s_kk_usercfg_h"),
        };
        let mut on = block.parts[part_index(part)];
        if widgets::switch_row_help(ui, scheme, &part_name(part), &help, &mut on).changed() {
            block.parts[part_index(part)] = on;
            block.asking = false;
        }
    }
    ui.add_space(6.0);
    let reason = block.blocked(&paths, &channels);
    if block.asking && reason.is_none() {
        let question = tf(
            "s_kk_frage",
            &[
                ("0", Arg::from(block.parts_line())),
                ("1", Arg::from(channel_of(&channels, &block.source))),
                ("2", Arg::from(channel_of(&channels, &block.target))),
            ],
        );
        ui.add(egui::Label::new(question).wrap());
        ui.horizontal_wrapped(|row| {
            if row.add(widgets::Button::new(&t("s_kk_ja"), ButtonKind::Strong, scheme).icon("copy")).clicked() {
                block.asking = false;
                block.run(&paths, &channels);
            }
            if row.add(widgets::Button::new(&t("abbrechen"), ButtonKind::Normal, scheme)).clicked() {
                block.asking = false;
            }
        });
    } else {
        block.asking = false;
        let label = t("s_kk_kopieren");
        let button = widgets::Button::new(&label, ButtonKind::Strong, scheme)
            .icon("copy")
            .blocked(reason.as_deref());
        if ui.add(button).clicked() {
            block.said = None;
            block.asking = true;
        }
    }
    if let Some((text, bad)) = &block.said {
        let tone = if *bad { scheme.red_text() } else { scheme.sub };
        ui.add(egui::Label::new(RichText::new(text).color(color(tone))).wrap());
    }
}
