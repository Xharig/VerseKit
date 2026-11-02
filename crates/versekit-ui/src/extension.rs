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

//! The docking point of the window: a program that embeds the window can
//! add one fixed entry to the sidebar, with pages of its own, and take over
//! which data and game folder the window works on.
//!
//! The entry sits directly under the overview, above the menu filter, with a
//! picture of the embedding program instead of an icon, and the filter never
//! hides it. Its pages are drawn by the [`Extension`]; what they want done to
//! the window they hand back as [`Command`]s, which the window carries out
//! after the frame. Nothing here knows what an extension does.

use egui::Ui;
use versekit_core::paths::Paths;
use versekit_core::theme::Scheme;

use crate::org_stock_page::Link;

/// What the window tells the pages of an extension. Read only.
pub struct Ctx<'a> {
    pub scheme: &'a Scheme,
    /// The data the window works on, when it has any.
    pub paths: Option<&'a Paths>,
    /// Seconds since the start, as `ui.input(|i| i.time)`.
    pub time: f64,
    /// Size of the interface in points (0 … 7).
    pub size: i64,
    /// Whether the interface is in English.
    pub english: bool,
    /// Whether the hint for a new version shows.
    pub update_bar: bool,
    /// Whether the new sign of the sidebar shows.
    pub new_mark: bool,
    pub test_build: bool,
    pub errors: u32,
    /// The pop-up of the overlay: on, and how many seconds.
    pub popup: bool,
    pub popup_seconds: i64,
    /// How far the window's content sticks out: right, bottom, and the tiles.
    pub overflow: [f32; 3],
}

/// What an extension asks the window to do.
pub enum Command {
    /// Start the program again with these arguments; this window closes once the other runs.
    Restart(Vec<String>),
    /// Read the game logs of other folders and write the settings under them from now on.
    UsePaths(Box<Paths>),
    /// The storage page takes this connection to the Basetool.
    UseStorageLink(Box<Link>),
    /// The connection page and the storage page talk to the real Basetool, as in the window of a
    /// user, even on a data folder that was given from outside. The credentials have a place of
    /// their own under `keyname` (Windows: a key of that name; elsewhere a file in a folder beside
    /// the one of the user's installation), never in the data folder and never in the place of the
    /// user's own login. `book_lock`: when given, nothing is booked and the sentence is the reason
    /// shown at the booking buttons.
    UseRealBasetool {
        keyname: String,
        book_lock: Option<String>,
    },
    /// A color scheme by its id.
    SetScheme(&'static str),
    SetSize(i64),
    SetEnglish(bool),
    /// Show the hint for a new version (`true`) or take it away.
    ShowUpdateBar(bool),
    /// The version the hint for a new version names; empty: no hint.
    SetUpdateVersion(String),
    /// The setup assistant starts again.
    OpenWizard,
    /// The new sign of the sidebar shows again.
    ResetNewMark,
    SetTestBuild(bool),
    SetErrorCount(u32),
    /// Read the game logs again for the mission log.
    Rescan,
    /// Open a page by its key.
    OpenPage(String),
    SetOverlayPopup {
        on: bool,
        seconds: i64,
    },
    /// Every text of the language file shows its key.
    ShowTextKeys(bool),
    /// A short notice at the bottom of the window.
    Notice(String),
}

/// The pages an embedding program adds to the window.
pub trait Extension {
    /// The name on the fixed entry.
    fn name(&self) -> String;

    /// The picture on the fixed entry: the bytes of a PNG file.
    fn picture(&self) -> &'static [u8];

    /// The pages: `(key, Lucide icon)`. The key is also the name shown.
    fn pages(&self) -> &'static [(&'static str, &'static str)];

    /// What the menu filter finds on a page besides its name.
    fn infos(&self, _page: &str) -> &'static [&'static str] {
        &[]
    }

    /// Draws one of the pages.
    fn show(&mut self, ui: &mut Ui, page: &str, ctx: &Ctx<'_>);

    /// Start (`begin`) and end of a frame of the window, the current page named.
    fn frame(&mut self, _ui: &Ui, _page: &str, _begin: bool) {}

    /// A mark for the head bar, drawn after the version; returns the page a click on it opens.
    fn head(&mut self, _ui: &mut Ui, _scheme: &Scheme) -> Option<String> {
        None
    }

    /// What the pages asked for since the last call.
    fn take_commands(&mut self) -> Vec<Command>;
}

/// The extension of a window and what the sidebar remembers of it.
pub(crate) struct Slot {
    pub(crate) extension: Box<dyn Extension>,
    /// Whether the pages under the entry are shown.
    pub(crate) open: bool,
}

impl Slot {
    pub(crate) fn new(extension: Box<dyn Extension>) -> Slot {
        Slot { extension, open: false }
    }

    /// Whether `page` is one of the pages.
    pub(crate) fn has(&self, page: &str) -> bool {
        self.extension.pages().iter().any(|(key, _)| *key == page)
    }

    /// The picture of the entry as an egui image.
    pub(crate) fn picture(&self) -> egui::Image<'static> {
        // The address follows the bytes, so another picture is not served from the cache of the old.
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.extension.picture().hash(&mut hasher);
        egui::Image::from_bytes(format!("bytes://extension/{:016x}.png", hasher.finish()), self.extension.picture())
    }
}
