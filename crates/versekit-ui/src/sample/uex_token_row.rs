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

//! The row of the player's own UEX token on the page General (`versekit_core::uex_token`): a covered field, the
//! buttons to save, test and remove it, and one line that says how it stands.
//!
//! The field never holds the stored token: once saved it is empty again and says so in its hint, so neither the
//! streamer mode nor a look over the shoulder can show it.

use egui::{RichText, Ui};
use versekit_core::basetool::secrets::SystemSecrets;
use versekit_core::language::{Arg, t, tf};
use versekit_core::paths::Paths;
use versekit_core::theme::Scheme;
use versekit_core::uex_token::{self, Check, Token};

use crate::color;
use crate::settings::CONTROL_WIDTH;
use crate::trade::{Job, Polled};
use crate::widgets::{self, ButtonKind};

/// What a click in the row asks for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Wish {
    /// Store the typed token.
    Save(Token),
    /// The typed text is no token.
    Invalid,
    /// Ask UEX with the typed token, or the stored one.
    Test,
    /// Delete the stored token.
    Remove,
}

/// The state of the row.
#[derive(Default)]
pub(super) struct UexTokenRow {
    /// What is typed into the field; never the stored token.
    typed: String,
    /// A token is stored and in use.
    stored: bool,
    /// The last thing said: the text and whether it is bad news.
    said: Option<(String, bool)>,
    /// The running test of the connection.
    checking: Option<Job<Check>>,
}

impl UexTokenRow {
    /// Takes over whether a token is in use.
    pub fn load(&mut self) {
        self.stored = uex_token::is_set();
    }

    /// The line under the controls, only as an answer to a click or for a refused token: a test running, a refused
    /// token, else the last thing said. Nothing at rest (the field says whether one is stored).
    fn status(&self) -> Option<(String, bool)> {
        if self.checking.is_some() {
            return Some((t("s_uex_prueft"), false));
        }
        if self.stored && uex_token::refused() {
            return Some((t("s_uex_abgelehnt"), true));
        }
        self.said.clone()
    }

    /// The controls in the column of controls. `copy`: the program runs on a copy of the data, which keeps no token.
    pub fn control(&mut self, row: &mut Ui, scheme: &Scheme, copy: bool) -> Option<Wish> {
        if copy {
            row.label(RichText::new(t("s_uex_kopie")).color(color(scheme.sub)));
            return None;
        }
        self.poll();
        let mut wish = None;
        row.vertical(|column| {
            let hint = t(if self.stored { "s_uex_feld_gespeichert" } else { "s_uex_feld" });
            let field = widgets::text_field(&mut self.typed, &hint, scheme)
                .password(true)
                .char_limit(1024)
                .desired_width(CONTROL_WIDTH);
            let field = column.add(field);
            // The name a screen reader says; egui gives the field only its hint, which changes.
            let spoken = t("s_uex_token");
            column.ctx().accesskit_node_builder(field.id, |node| node.set_label(spoken.as_str()));
            let typed_enter = field.lost_focus() && column.input(|i| i.key_pressed(egui::Key::Enter));
            let has_text = !self.typed.trim().is_empty();
            let (save, test, remove) = (t("s_uex_speichern"), t("s_uex_testen"), t("s_uex_entfernen"));
            let (first, none_stored, busy) = (t("s_uex_erst_einfuegen"), t("s_uex_keiner"), t("s_uex_prueft"));
            // Always the same three buttons in the same place; one that cannot act now is grey and says why.
            column.horizontal(|buttons| {
                let blocked = (!has_text).then_some(first.as_str());
                let button = widgets::Button::new(&save, ButtonKind::Normal, scheme).blocked(blocked);
                if (buttons.add(button).clicked() || typed_enter) && has_text {
                    wish = Some(Token::parse(&self.typed).map_or(Wish::Invalid, Wish::Save));
                }
                let blocked = (!self.stored).then_some(none_stored.as_str());
                let button = widgets::Button::new(&remove, ButtonKind::Danger, scheme).blocked(blocked);
                if buttons.add(button).clicked() && self.stored {
                    wish = Some(Wish::Remove);
                }
            });
            let blocked = if self.checking.is_some() {
                Some(busy.as_str())
            } else if !has_text && !self.stored {
                Some(first.as_str())
            } else {
                None
            };
            let button = widgets::Button::new(&test, ButtonKind::Normal, scheme).blocked(blocked);
            if column.add(button).clicked() && blocked.is_none() {
                wish = Some(Wish::Test);
            }
            if let Some((text, bad)) = self.status() {
                let tint = if bad { scheme.red } else { scheme.sub };
                column.label(RichText::new(text).color(color(tint)));
            }
        });
        wish
    }

    /// Takes the answer of a finished test.
    fn poll(&mut self) {
        let Some(job) = &self.checking else { return };
        match job.poll() {
            Polled::Running => {}
            Polled::Done(found) => {
                self.checking = None;
                self.said = Some(said_of(&found));
            }
            Polled::Lost => self.checking = None,
        }
    }

    /// Does what a click asked for. `real`: the program on its own data, whose store of secrets is written; the
    /// sample page only shows what would happen. `ctx` wakes the page when a test is done.
    pub fn apply(&mut self, wish: Wish, real: Option<&Paths>, ctx: &egui::Context) {
        match wish {
            Wish::Invalid => self.said = Some((t("s_uex_ungueltig"), true)),
            Wish::Save(token) => {
                let saved = match real {
                    Some(paths) => uex_token::save(&SystemSecrets::from_host(&paths.host), &token),
                    None => Ok(()),
                };
                self.said = Some(match saved {
                    Ok(()) => {
                        self.typed.clear();
                        self.stored = true;
                        (t("s_uex_gespeichert"), false)
                    }
                    Err(error) => (tf("s_uex_nicht_gespeichert", &[("0", Arg::from(error.code))]), true),
                });
            }
            Wish::Remove => {
                let removed = match real {
                    Some(paths) => uex_token::forget(&SystemSecrets::from_host(&paths.host)),
                    None => Ok(()),
                };
                self.stored = false;
                self.said = Some(match removed {
                    Ok(()) => (t("s_uex_entfernt"), false),
                    Err(error) => (tf("s_uex_nicht_gespeichert", &[("0", Arg::from(error.code))]), true),
                });
            }
            Wish::Test => {
                let token = match Token::parse(&self.typed) {
                    Some(token) => Some(token),
                    None if !self.typed.trim().is_empty() => {
                        self.said = Some((t("s_uex_ungueltig"), true));
                        return;
                    }
                    None => uex_token::current(),
                };
                // The sample page keeps no token: it shows the answer a test would give.
                if real.is_none() {
                    if token.is_some() || self.stored {
                        self.said = Some(said_of(&Check::Accepted));
                    }
                    return;
                }
                let Some(token) = token else { return };
                let agent = crate::trade::user_agent();
                self.said = None;
                self.checking = Some(Job::spawn(ctx, "uex-token-test", move || uex_token::check_live(&token, &agent)));
            }
        }
    }
}

/// What the row says of a test.
fn said_of(found: &Check) -> (String, bool) {
    match found {
        Check::Accepted => (t("s_uex_angenommen"), false),
        Check::Refused => (t("s_uex_abgelehnt"), true),
        Check::Offline => (t("s_uex_offline"), true),
        Check::Failed(what) => (tf("s_uex_fehler", &[("0", Arg::from(what.as_str()))]), true),
    }
}

#[cfg(test)]
mod tests;
