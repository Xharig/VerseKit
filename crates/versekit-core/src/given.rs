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

//! Folders that were given from outside and are not the user's to change.
//!
//! The page "Installation & Pfade" reads them from [`Paths::given`] and then
//! shows who decided, and offers neither choosing nor moving: a move must
//! never land in a copy of the data or in someone's real data by accident.
//!
//! Set by:
//!
//! * [`Paths::for_copy`] (the start option `--daten`): the data folder, by
//!   [`GivenBy::CommandLine`]; the overlay's start option `--archiv` the
//!   archive, the same way;
//! * the environment variables [`crate::legacy::ENV_HOME`] (data folder) and
//!   [`crate::paths::ENV_INSTALL_DIR`] (game folder), found by [`Paths::given`] itself;
//! * another part of the program, through [`Paths::set_given_data`] and
//!   [`Paths::set_given_game`] and [`Paths::set_given_archive`] with a name for
//!   [`GivenBy::Named`].

/// Who decided a folder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GivenBy {
    /// A start option (`--daten`, the overlay's `--archiv`).
    CommandLine,
    /// An environment variable, by its name.
    Environment(String),
    /// Another part of the program, by the name it is shown under.
    Named(String),
}

/// A folder that is fixed, and who fixed it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fixed {
    pub folder: String,
    pub by: GivenBy,
}

/// What is fixed from outside; each part is `None` while the user decides.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Given {
    /// The data folder.
    pub data: Option<Fixed>,
    /// The game folder (LIVE); the others lie next to it.
    pub game: Option<Fixed>,
    /// The `Data.p4k`, where it does not lie in the game folder.
    pub archive: Option<Fixed>,
}
