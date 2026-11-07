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

//! Why game data could not be read.

use std::fmt;
use std::io;
use std::path::PathBuf;

/// The reader's own error, kept only as the cause.
pub type Cause = Box<dyn std::error::Error + Send + Sync + 'static>;

/// A failure to read game data.
#[derive(Debug)]
#[non_exhaustive]
pub enum Error {
    /// No game folder was found.
    NoGameFolder,
    /// The archive could not be opened or an entry not be opened in it.
    Archive { path: PathBuf, source: Cause },
    /// The archive has no entry at this path.
    MissingEntry { entry: String },
    /// The entry is larger than what is read into memory.
    TooLarge { entry: String, size: u64 },
    /// The data of the entry could not be read.
    Read { entry: String, source: io::Error },
    /// The DataForge in the entry could not be opened.
    DataForge { entry: String, source: Cause },
    /// A `global.ini` could not be decoded or indexed.
    Localization { entry: String, source: Cause },
    /// The entry is encrypted. VerseKit never brings nor asks for a key: the
    /// game data of this channel stay unread.
    Encrypted { entry: String },
    /// p4kx could not build its catalog of the contracts, or its JSON could
    /// not be read.
    Contracts { source: Cause },
    /// p4kx could not build its catalog of the ships and their parts, or its JSON could not be read.
    Slots { source: Cause },
}

impl Error {
    /// Whether the entry is encrypted (see [`Error::Encrypted`]).
    pub const fn is_encrypted(&self) -> bool {
        matches!(self, Self::Encrypted { .. })
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoGameFolder => f.write_str("no game folder found"),
            Self::Archive { path, .. } => write!(f, "cannot read the archive {}", path.display()),
            Self::MissingEntry { entry } => write!(f, "the archive has no entry {entry}"),
            Self::TooLarge { entry, size } => write!(f, "the entry {entry} is too large ({size} bytes)"),
            Self::Read { entry, .. } => write!(f, "cannot read the entry {entry}"),
            Self::DataForge { entry, .. } => write!(f, "cannot open the DataForge in {entry}"),
            Self::Localization { entry, .. } => write!(f, "cannot read the texts in {entry}"),
            Self::Encrypted { entry } => {
                write!(f, "the entry {entry} is encrypted; VerseKit reads no encrypted game data")
            }
            Self::Contracts { .. } => f.write_str("cannot read the contracts of the game data with p4kx"),
            Self::Slots { .. } => f.write_str("cannot read the slots of the ships of the game data with p4kx"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::NoGameFolder | Self::MissingEntry { .. } | Self::TooLarge { .. } | Self::Encrypted { .. } => None,
            Self::Archive { source, .. }
            | Self::DataForge { source, .. }
            | Self::Localization { source, .. }
            | Self::Contracts { source }
            | Self::Slots { source } => Some(source.as_ref()),
            Self::Read { source, .. } => Some(source),
        }
    }
}
