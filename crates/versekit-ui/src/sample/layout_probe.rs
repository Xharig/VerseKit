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

//! What the layout check of all pages (`tests/layout_guard.rs`) needs to know about the window.

use super::{NAV, SamplePage, TOP_PAGE};

/// Pages the window opens that stand in no group of the sidebar.
const OUTSIDE_THE_SIDEBAR: [&str; 3] = ["hf_wasistneu", "hf_diagnose", super::ASOP_PAGE];

impl SamplePage {
    /// The key of every page of the main window: the overview (empty key) first, then the page at the top
    /// of the sidebar, the pages of its groups in their order and the pages that only a link opens.
    pub fn page_keys() -> Vec<&'static str> {
        let mut keys = vec!["", TOP_PAGE.0];
        keys.extend(NAV.iter().flat_map(|(_, pages)| pages.iter().map(|(key, _)| *key)));
        keys.extend(OUTSIDE_THE_SIDEBAR);
        keys
    }

    /// Chooses the first entry of the list of the open page, as a click on it does, where a chosen entry opens
    /// its detail beside the list or under its row. Whether the page has such a list with an entry in it.
    pub fn choose_first_entry(&mut self) -> bool {
        match self.page.as_str() {
            "hf_liste" => self.blueprint_view.choose_first(),
            "hf_auftragslog" => self.contract_view.choose_first(),
            "hf_herstellung" => self.crafting.choose_first(),
            "hf_bergbau" => self.mining.choose_first(),
            _ => false,
        }
    }
}
