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

//! Writes a page holds back for a moment (4.12 `before_close`): a page that writes after a pause signs up with
//! [`WritesOnClose`], and the main window writes them all before its settings when it closes or starts anew
//! (`SamplePage::save_and_wait`).

use versekit_core::paths::Paths;

/// A part of the window that writes after a pause and owes that write when the window closes.
pub trait WritesOnClose {
    /// Writes what is held back, now. `paths` are the settings of the window, `None` without a settings file.
    fn write_held_back(&mut self, paths: Option<&Paths>);
}

impl WritesOnClose for crate::notes_page::NotesState {
    fn write_held_back(&mut self, _paths: Option<&Paths>) {
        self.flush(None);
    }
}

impl WritesOnClose for crate::startprograms_page::StartProgramsState {
    fn write_held_back(&mut self, paths: Option<&Paths>) {
        if let Some(paths) = paths {
            self.flush(paths);
        }
    }
}

impl WritesOnClose for crate::ship_names::ShipNames {
    fn write_held_back(&mut self, _paths: Option<&Paths>) {
        self.write_left();
    }
}

impl WritesOnClose for crate::hangar_page::HangarState {
    fn write_held_back(&mut self, paths: Option<&Paths>) {
        self.ship_names().write_held_back(paths);
    }
}

/// Writes of parts that sign up while the window runs, kept by the window until it closes.
#[derive(Default)]
pub struct SignedUp {
    writers: Vec<Box<dyn WritesOnClose>>,
}

impl SignedUp {
    pub fn sign_up(&mut self, writer: Box<dyn WritesOnClose>) {
        self.writers.push(writer);
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut dyn WritesOnClose> {
        self.writers.iter_mut().map(|w| w.as_mut() as &mut dyn WritesOnClose)
    }
}
