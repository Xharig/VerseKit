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

//! The choice of the installation for pages that show or change a file of the game: one button per installed
//! channel, nothing when there is only one. The channels are the ones the build knows (`paths::CHANNELS`): a
//! player's build never lists a channel under a non-disclosure agreement.
//!
//! The choice is the folder of the installation; `None` is the main installation, the one all other pages use.

use egui::Ui;
use versekit_core::language::t;
use versekit_core::paths::Paths;
use versekit_core::theme::Scheme;

use crate::settings;
use crate::widgets::{self, ButtonKind};

/// One installation that can be chosen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Install {
    /// The channel (`LIVE`, `PTU`).
    pub name: String,
    /// The game folder.
    pub folder: String,
}

/// The installations there are, in the order of the channels. Empty where none is found.
pub fn installs(paths: &Paths) -> Vec<Install> {
    paths.installed_channels().into_iter().map(|(name, folder, _)| Install { name, folder }).collect()
}

/// The folder the choice stands for: `None` for the main installation (also where the chosen folder is not
/// installed any more), else the folder chosen.
pub fn folder(paths: &Paths, choice: &Option<String>) -> Option<String> {
    let chosen = choice.as_deref()?;
    let main = paths.game_folder();
    (main.as_deref() != Some(chosen) && installs(paths).iter().any(|i| i.folder == chosen)).then(|| chosen.to_string())
}

/// The buttons, in a row of a card. Gives whether the choice changed. With `locked` the buttons wait (changes
/// that belong to one installation must not slip over to another).
pub fn show(ui: &mut Ui, scheme: &Scheme, paths: &Paths, choice: &mut Option<String>, locked: bool) -> bool {
    let all = installs(paths);
    if all.len() < 2 {
        return false;
    }
    let main = paths.game_folder();
    let shown = folder(paths, choice).or(main.clone());
    let mut picked: Option<Install> = None;
    let hint = if locked { t("s_inst_erst_schreiben") } else { t("s_inst_hilfe") };
    widgets::card(ui, scheme, |ui| {
        settings::setting_row(ui, scheme, &t("s_inst_titel"), &hint, false, |row| {
            row.add_enabled_ui(!locked, |row| {
                for install in &all {
                    let on = shown.as_deref() == Some(install.folder.as_str());
                    let kind = if on { ButtonKind::Strong } else { ButtonKind::Normal };
                    if row.add(widgets::Button::new(&install.name, kind, scheme)).clicked() && !on {
                        picked = Some(install.clone());
                    }
                }
            });
        });
    });
    let Some(install) = picked else { return false };
    *choice = (main.as_deref() != Some(install.folder.as_str())).then_some(install.folder);
    true
}
