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

//! Streamer mode: the switch of the settings page.

use versekit_core::paths::Paths;
use versekit_core::streamer;

use super::SamplePage;
use super::real_data::{folder, host_in};

/// The switch is off from the factory and is kept in the settings file.
#[test]
fn the_switch_is_off_from_the_factory_and_is_stored() {
    let _language = crate::language_lock();
    let dir = folder("streamer-schalter");
    let mut page = SamplePage::with_settings(Paths::new(host_in(&dir, &[])));
    assert!(!page.streamer, "off from the factory");
    assert!(!streamer::load(&Paths::new(host_in(&dir, &[]))));
    let values = page.setting_values();
    assert!(
        values.iter().any(|(key, value)| *key == streamer::SETTING && *value == versekit_core::jsonio::Value::Bool(false)),
        "the setting is written with the others"
    );
    page.streamer = true;
    assert!(
        page.setting_values()
            .iter()
            .any(|(key, value)| *key == streamer::SETTING && *value == versekit_core::jsonio::Value::Bool(true))
    );
    let _ = std::fs::remove_dir_all(dir);
}
