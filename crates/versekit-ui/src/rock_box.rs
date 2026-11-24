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

//! The card "Does the rock pay?" in the tab Equipment of the page Mining, below the cards of the mining ships of
//! the hangar: which of them the answer is computed for and what it has fitted (heads and modules live in the
//! hangar and are changed in its card), the gadget on the rock and the active module in use, the two lines of the
//! overlay switched on or off, and a field to try the answer with a mass and a resistance typed in.
//!
//! Every choice is stored at once; the overlay reads the choice and the hangar again by itself within seconds.

use std::time::SystemTime;

use egui::{RichText, Ui};
use versekit_core::jsonio::Value;
use versekit_core::language::t;
use versekit_core::mining_gear::{self, Choice, Gear, HangarShip};
use versekit_core::paths::Paths;
use versekit_core::rock_crack::{self, Rock};
use versekit_core::theme::Scheme;

use crate::{color, widgets};

/// How often the files of the equipment are looked at, in seconds.
const LOOK_SECONDS: f64 = 2.0;

/// The card's state.
pub struct RockBox {
    paths: Option<Paths>,
    pub gear: Gear,
    pub choice: Choice,
    pub show: (bool, bool),
    seen: Option<SystemTime>,
    looked_at: f64,
    /// The values typed to try the answer.
    pub mass: String,
    pub resistance: String,
}

impl Default for RockBox {
    fn default() -> RockBox {
        RockBox {
            paths: None,
            gear: Gear::default(),
            choice: Choice::default(),
            show: (true, true),
            seen: None,
            looked_at: f64::NEG_INFINITY,
            mass: String::new(),
            resistance: String::new(),
        }
    }
}

fn changed_at(paths: &Paths) -> Option<SystemTime> {
    let time = |name: &str| std::fs::metadata(paths.app_file(name)).and_then(|m| m.modified()).ok();
    [time(mining_gear::FILE), time(versekit_core::erkul_slots::GAME_SHIPS_FILE)].into_iter().flatten().max()
}

impl RockBox {
    pub fn with_files(paths: &Paths) -> RockBox {
        let paths = Paths::new(paths.host.clone());
        let mut state = RockBox {
            choice: Choice::read(&paths),
            show: (
                paths.setting_bool(mining_gear::keys::SHOW_VERDICT, true),
                paths.setting_bool(mining_gear::keys::SHOW_WORTH, true),
            ),
            ..RockBox::default()
        };
        state.gear = Gear::load(&paths);
        state.seen = changed_at(&paths);
        state.paths = Some(paths);
        state
    }

    /// Reads the equipment again when its files changed (the game data were read anew).
    fn follow(&mut self, now: f64) {
        if now - self.looked_at < LOOK_SECONDS {
            return;
        }
        self.looked_at = now;
        let Some(paths) = &self.paths else { return };
        let seen = changed_at(paths);
        if seen != self.seen {
            self.seen = seen;
            self.gear = Gear::load(paths);
        }
    }

    fn store_choice(&self) {
        if let Some(paths) = &self.paths {
            for (key, value) in self.choice.setting_values() {
                paths.set_setting(key, value);
            }
        }
    }

    fn store_show(&self) {
        if let Some(paths) = &self.paths {
            paths.set_setting(mining_gear::keys::SHOW_VERDICT, Value::Bool(self.show.0));
            paths.set_setting(mining_gear::keys::SHOW_WORTH, Value::Bool(self.show.1));
        }
    }

    /// The fitting of the chosen ship among the mining ships of the hangar (the first one where the choice names
    /// none of them).
    pub fn follow_ships(&mut self, ships: &[HangarShip]) {
        self.choice = std::mem::take(&mut self.choice).with_ship(ships);
    }

    /// The name of a part of the equipment, else its identifier.
    fn name_of(&self, reference: &str) -> String {
        let gear = &self.gear;
        gear.head(reference)
            .map(|h| h.name.clone())
            .or_else(|| gear.module(reference).map(|m| m.name.clone()))
            .unwrap_or_else(|| reference.to_string())
    }

    /// What the chosen ship has fitted, a line per head with its place among the lasers of the ship:
    /// `Helix II: Focus III, Rieger-C3, Rieger-C3`.
    pub fn fitted(&self) -> Vec<(usize, String)> {
        self.choice
            .lasers
            .iter()
            .enumerate()
            .filter(|(_, chosen)| !chosen.head.is_empty())
            .map(|(place, chosen)| {
                let head = self.name_of(&chosen.head);
                let line = if chosen.modules.is_empty() {
                    head
                } else {
                    let modules: Vec<String> = chosen.modules.iter().map(|m| self.name_of(m)).collect();
                    format!("{head}: {}", modules.join(", "))
                };
                (place, line)
            })
            .collect()
    }

    /// The lines of [`fitted`](Self::fitted).
    pub fn fitted_lines(&self) -> Vec<String> {
        self.fitted().into_iter().map(|(_, line)| line).collect()
    }

    /// The answer for the values typed in, with the choice; `None` until both are numbers.
    pub fn trial(&self) -> Option<String> {
        let number = |text: &str| text.trim().replace(',', ".").trim_end_matches('%').trim().parse::<f64>().ok();
        // The mass is whole kilograms: `150.000` and `150,000` are thousands.
        let mass = versekit_core::stock::normalize_separators(&self.mass, true).parse::<f64>().ok()?;
        let rock = Rock { mass: Some(mass), resistance: Some(number(&self.resistance)?), instability: None };
        let loadout = self.choice.loadout(&self.gear);
        let verdict = rock_crack::assess(&rock, &loadout);
        Some(rock_crack::verdict_line_in(versekit_core::language::current(), &verdict, &loadout))
    }
}

/// A drop-down of `choices` (value, label) with a name for screen readers; the value picked anew.
fn choose(ui: &mut Ui, salt: (&str, usize), name: &str, current: &str, choices: &[(String, String)]) -> Option<String> {
    let shown = choices.iter().find(|(value, _)| value == current).map_or_else(|| t("s_stein_keins"), |(_, l)| l.clone());
    let mut picked = current.to_string();
    let combo = egui::ComboBox::from_id_salt(salt).selected_text(&shown).show_ui(ui, |list| {
        for (value, label) in choices {
            list.selectable_value(&mut picked, value.clone(), label);
        }
    });
    widgets::name_combo(&combo.response, name, &shown);
    (picked != current).then_some(picked)
}

fn labelled(ui: &mut Ui, scheme: &Scheme, label: &str, add: impl FnOnce(&mut Ui)) {
    ui.horizontal_wrapped(|row| {
        row.label(RichText::new(label).color(color(scheme.sub)));
        add(row);
    });
}

/// The card, below the cards of the mining ships of the hangar (`ships`): which of them counts, what it has
/// fitted (changed in its card above), the gadget and the active module in use.
///
/// Gives whether the link to the tab Signatur was clicked (shown while the scanner is off).
pub fn show_box(ui: &mut Ui, scheme: &Scheme, state: &mut RockBox, ships: &[HangarShip], scanner_on: bool, now: f64) -> bool {
    state.follow(now);
    state.follow_ships(ships);
    let mut to_signature = false;
    widgets::card(ui, scheme, |ui| {
        widgets::card_title(ui, scheme, &t("s_stein_kopf"));
        ui.add(egui::Label::new(RichText::new(t("s_stein_lead")).color(color(scheme.sub))).wrap());
        if !scanner_on {
            widgets::hint(ui, scheme, &t("s_stein_scanner_aus"));
            let link = ui.add(egui::Link::new(RichText::new(t("s_stein_zum_reiter")).color(color(scheme.accent))));
            to_signature = link.on_hover_cursor(egui::CursorIcon::PointingHand).clicked();
        }
        if state.gear.is_empty() {
            widgets::hint(ui, scheme, &t("s_stein_keine_daten"));
            return;
        }
        ui.add_space(6.0);
        let none = || (String::new(), t("s_stein_keins"));
        let mut changed = false;
        if ships.len() > 1 {
            let choices: Vec<(String, String)> = ships.iter().map(|s| (s.key.clone(), s.name.clone())).collect();
            labelled(ui, scheme, &t("s_stein_schiff"), |row| {
                if let Some(key) = choose(row, ("stein-schiff", 0), &t("s_stein_schiff"), &state.choice.ship, &choices) {
                    state.choice.ship = key;
                    state.follow_ships(ships);
                    changed = true;
                }
            });
        } else if let Some(ship) = ships.first() {
            labelled(ui, scheme, &t("s_stein_schiff"), |row| {
                row.label(RichText::new(&ship.name).strong());
            });
        }
        let fitted = state.fitted();
        if !fitted.is_empty() {
            ui.label(RichText::new(t("s_stein_feuern")).small().color(color(scheme.sub)));
        }
        for (place, line) in fitted {
            let mut fires = state.choice.fires(place);
            if ui.checkbox(&mut fires, RichText::new(line).small()).changed() {
                state.choice.set_fires(place, fires);
                changed = true;
            }
        }
        let gadgets: Vec<(String, String)> = std::iter::once(none())
            .chain(state.gear.gadgets.iter().map(|g| (g.reference.clone(), g.name.clone())))
            .collect();
        labelled(ui, scheme, &t("s_stein_gadget"), |row| {
            if let Some(gadget) = choose(row, ("stein-gadget", 0), &t("s_stein_gadget"), &state.choice.gadget, &gadgets) {
                state.choice.gadget = gadget;
                changed = true;
            }
        });
        let active: Vec<(String, String)> = std::iter::once(none())
            .chain(state.gear.modules.iter().filter(|m| m.active).map(|m| (m.reference.clone(), m.name.clone())))
            .collect();
        labelled(ui, scheme, &t("s_stein_aktiv"), |row| {
            if let Some(module) = choose(row, ("stein-aktiv", 0), &t("s_stein_aktiv"), &state.choice.active, &active) {
                state.choice.active = module;
                changed = true;
            }
        });
        if changed {
            state.store_choice();
        }
        ui.add_space(6.0);
        let (mut verdict, mut worth) = state.show;
        let a = widgets::switch_row(ui, scheme, &t("s_stein_zeile_aufbrechbar"), &mut verdict).changed();
        let b = widgets::switch_row(ui, scheme, &t("s_stein_zeile_wert"), &mut worth).changed();
        if a || b {
            state.show = (verdict, worth);
            state.store_show();
        }
        ui.add_space(6.0);
        ui.label(RichText::new(t("s_stein_probe")).strong());
        ui.horizontal_wrapped(|row| {
            row.add(widgets::text_field(&mut state.mass, &t("s_stein_masse"), scheme).desired_width(110.0));
            row.add(widgets::text_field(&mut state.resistance, &t("s_stein_widerstand"), scheme).desired_width(110.0));
        });
        if let Some(line) = state.trial() {
            ui.label(RichText::new(line).color(color(scheme.accent)));
        }
    });
    to_signature
}

#[cfg(test)]
mod tests;
