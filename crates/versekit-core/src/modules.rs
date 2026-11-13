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

//! Modules (`modules.py`): pages of the menu switched off, each module for itself whatever group of the
//! menu its pages stand in; a group without a visible page vanishes.
//!
//! ⚠⚠ The rules that matter:
//! 1. **Everything is on at the start.** Whoever has Verse-Kit new sees all it can do.
//! 2. **Blueprints, settings and info can never be switched off**: the core of the tool and the way to the
//!    error report. Their pages are not in [`SWITCHABLE`]; only the connection to the Basetool in the
//!    settings follows the module Basetool ([`ALSO_HIDES`] for the unit storage). The statistics can be
//!    hidden; they are evaluated all the same as long as "evaluate automatically" is on, otherwise the weeks
//!    in between were missing when it is switched on again.
//! 3. **Hide, do not switch off.** Nothing is deleted or stopped; the data stay.
//! 4. **A jump to a hidden page** (from the blueprint list to Crafting, say) lands on "Module" and says
//!    what is off, instead of doing nothing.
//!
//! The pages are named by the keys of the menu of Verse-Kit (`hf_…`), not by the ids of 4.12.

use crate::jsonio::Value;
use crate::paths::Paths;

/// Module → its pages. Every one has an entry in the menu.
///
/// ⚠ The key `modul_basetool` is also the switch of the Basetool area of the overview: one switch for both
/// views.
pub const SWITCHABLE: [(&str, &[&str]); 8] = [
    ("basetool", &["k_basetool_verbindung", "hf_bedarf", "hf_bank"]),
    ("schiffe", &["hf_hangar"]),
    ("werkstatt", &["hf_herstellung", "hf_farmliste"]),
    ("bergbau", &["hf_bergbau", "hf_raffinerien"]),
    ("bergung", &["hf_bergung"]),
    ("handel", &["hf_laeden", "hf_verkauf", "hf_routen"]),
    ("beschaffung", &["hf_einkaufsliste", "hf_contested"]),
    ("statistiken", &["hf_st_uebersicht", "hf_st_schiffe", "hf_st_auftraege", "hf_st_quantum", "hf_st_stabil"]),
];

/// Pages outside the group that a switch hides all the same, so that the page Modules names them too.
pub const ALSO_HIDES: [(&str, &[&str]); 1] = [("basetool", &["hf_staffellager"])];

/// The name of a module on the page Modules (a key of the texts).
pub fn label(group: &str) -> &'static str {
    match group {
        "basetool" => "k_basetool_titel",
        "schiffe" => "hf_gruppe_schiffe",
        "werkstatt" => "s_mo_werkstatt",
        "bergbau" => "s_mo_bergbau",
        "bergung" => "hf_gruppe_bergung",
        "handel" => "hf_gruppe_handel",
        "beschaffung" => "hf_gruppe_beschaffung",
        "statistiken" => "hf_gruppe_statistik",
        _ => "",
    }
}

/// The key in the settings.
pub fn key(group: &str) -> String {
    format!("modul_{group}")
}

/// The module whose switch held these pages before they got a module of their own: without a switch of its own
/// a module follows it, so nothing comes back that was hidden.
const FOLLOWS: [(&str, &str); 2] = [("bergbau", "werkstatt"), ("beschaffung", "schiffe")];

/// Is the module on? A group that cannot be switched off is always on. A module of [`FOLLOWS`] without a switch
/// of its own is as its former module.
pub fn enabled(paths: &Paths, group: &str) -> bool {
    if SWITCHABLE.iter().all(|(name, _)| *name != group) {
        return true;
    }
    let default = FOLLOWS
        .iter()
        .find(|(module, _)| *module == group)
        .is_none_or(|(_, former)| paths.setting_bool(&key(former), true));
    paths.setting_bool(&key(group), default)
}

/// Switches a module; true if it was written (a group that cannot be switched is never written).
pub fn set_enabled(paths: &Paths, group: &str, on: bool) -> bool {
    if SWITCHABLE.iter().all(|(name, _)| *name != group) {
        return false;
    }
    paths.set_setting(&key(group), Value::Bool(on))
}

/// The module a page belongs to, if it is one that can be switched (the pages of [`ALSO_HIDES`] too).
pub fn group_of(page: &str) -> Option<&'static str> {
    SWITCHABLE.iter().chain(ALSO_HIDES.iter()).find(|(_, pages)| pages.contains(&page)).map(|(group, _)| *group)
}

/// Every page whose module is off now (those of [`ALSO_HIDES`] too).
pub fn hidden_pages(paths: &Paths) -> Vec<&'static str> {
    SWITCHABLE
        .iter()
        .chain(ALSO_HIDES.iter())
        .filter(|(group, _)| !enabled(paths, group))
        .flat_map(|(_, pages)| pages.iter().copied())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::language;
    use crate::paths::Host;
    use crate::pypath::Flavor;

    fn scratch(name: &str) -> String {
        let dir = std::env::temp_dir().join(format!("versekit-modules-{}-{name}", crate::testdir::run_tag()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("folder");
        dir.to_string_lossy().replace('\\', "/")
    }

    fn paths_in(home: &str) -> Paths {
        Paths::new(Host::new(Flavor::Posix, &[("SC_BP_HOME", home), ("HOME", home)], home))
    }

    #[test]
    fn everything_is_on_at_the_start_and_a_switch_hides_exactly_its_pages() {
        let home = scratch("an");
        let paths = paths_in(&home);
        assert!(hidden_pages(&paths).is_empty(), "rule 1: all on");
        assert!(set_enabled(&paths, "handel", false));
        assert!(!enabled(&paths, "handel") && enabled(&paths, "schiffe"));
        assert_eq!(hidden_pages(&paths), ["hf_laeden", "hf_verkauf", "hf_routen"]);
        assert!(set_enabled(&paths, "basetool", false));
        let hidden = hidden_pages(&paths);
        assert!(hidden.contains(&"hf_staffellager") && hidden.contains(&"k_basetool_verbindung"), "{hidden:?}");
        assert!(set_enabled(&paths, "handel", true) && set_enabled(&paths, "basetool", true));
        assert!(hidden_pages(&paths).is_empty(), "and back: nothing was lost");
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn the_module_salvage_has_the_one_merged_page() {
        let home = scratch("bergung");
        let paths = paths_in(&home);
        assert_eq!(group_of("hf_bergung"), Some("bergung"));
        assert_eq!(group_of("hf_zerlegen"), None, "no page of its own any more");
        assert!(set_enabled(&paths, "bergung", false));
        assert_eq!(hidden_pages(&paths), ["hf_bergung"]);
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn the_core_of_the_tool_cannot_be_switched_off() {
        let home = scratch("kern");
        let paths = paths_in(&home);
        for group in ["bauplaene", "einstellungen", "info", "", "unsinn"] {
            assert!(enabled(&paths, group), "{group}");
            assert!(!set_enabled(&paths, group, false), "{group} is not written");
        }
        // A stored value for such a group is not read either.
        assert!(paths.set_setting("modul_info", Value::Bool(false)));
        assert!(enabled(&paths, "info"));
        for page in
            ["hf_liste", "hf_fortschritt", "hf_auftragslog", "hf_allgemein", "hf_ordner", "hf_module", "hf_ueber"]
        {
            assert_eq!(group_of(page), None, "{page}");
        }
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn a_page_belongs_to_one_module_and_every_module_has_a_name_and_pages() {
        let mut seen = Vec::new();
        for (group, pages) in SWITCHABLE.iter().chain(ALSO_HIDES.iter()) {
            for page in *pages {
                assert!(!seen.contains(page), "{page} in two modules");
                seen.push(*page);
                assert_eq!(group_of(page), Some(*group));
            }
        }
        for (group, pages) in SWITCHABLE {
            assert!(!pages.is_empty(), "{group}");
            let name = label(group);
            assert!(!name.is_empty() && language::t(name) != name, "{group} has no name: {name}");
        }
    }

    #[test]
    fn an_off_switch_is_stored_as_modul_name_and_a_text_of_4_12_reads_as_before() {
        let home = scratch("schluessel");
        let paths = paths_in(&home);
        assert!(set_enabled(&paths, "bergung", false));
        let stored = paths.settings();
        assert!(stored.iter().any(|(k, v)| k == "modul_bergung" && *v == Value::Bool(false)), "{stored:?}");
        // 4.12 wrote a plain truth value; a hand-made "nein" reads as off, a missing one as on.
        assert!(paths.set_setting("modul_schiffe", Value::str("nein")));
        assert!(!enabled(&paths, "schiffe"));
        assert!(enabled(&paths, "werkstatt"));
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn crafting_and_mining_are_two_modules_and_an_old_off_switch_keeps_both_off() {
        let home = scratch("werkstatt-bergbau");
        let paths = paths_in(&home);
        assert_eq!(group_of("hf_herstellung"), Some("werkstatt"));
        assert_eq!(group_of("hf_farmliste"), Some("werkstatt"));
        assert_eq!(group_of("hf_bergbau"), Some("bergbau"));
        assert_eq!(group_of("hf_raffinerien"), Some("bergbau"));
        // Crafting off, mining on: only the crafting pages go.
        assert!(set_enabled(&paths, "werkstatt", false) && set_enabled(&paths, "bergbau", true));
        assert_eq!(hidden_pages(&paths), ["hf_herstellung", "hf_farmliste"]);
        // Whoever had the old module off, before mining had a switch of its own, keeps mining off too.
        let old = scratch("werkstatt-alt");
        let before = paths_in(&old);
        assert!(before.set_setting("modul_werkstatt", Value::Bool(false)));
        assert!(!enabled(&before, "bergbau"));
        assert_eq!(hidden_pages(&before), ["hf_herstellung", "hf_farmliste", "hf_bergbau", "hf_raffinerien"]);
        // Mining switched on by itself stays on.
        assert!(set_enabled(&before, "bergbau", true));
        assert!(enabled(&before, "bergbau") && !enabled(&before, "werkstatt"));
        let _ = std::fs::remove_dir_all(home);
        let _ = std::fs::remove_dir_all(old);
    }

    #[test]
    fn procurement_is_a_module_of_its_own_and_follows_an_old_off_switch_of_the_ships() {
        let home = scratch("beschaffung");
        let paths = paths_in(&home);
        assert_eq!(group_of("hf_einkaufsliste"), Some("beschaffung"));
        assert_eq!(group_of("hf_contested"), Some("beschaffung"));
        assert_eq!(group_of("hf_hangar"), Some("schiffe"));
        assert!(set_enabled(&paths, "beschaffung", false));
        assert_eq!(hidden_pages(&paths), ["hf_einkaufsliste", "hf_contested"]);
        // The shopping list stood under the ships: whoever had them off keeps it off.
        let old = scratch("beschaffung-alt");
        let before = paths_in(&old);
        assert!(before.set_setting("modul_schiffe", Value::Bool(false)));
        assert!(!enabled(&before, "beschaffung"));
        assert!(set_enabled(&before, "beschaffung", true) && enabled(&before, "beschaffung"));
        let _ = std::fs::remove_dir_all(home);
        let _ = std::fs::remove_dir_all(old);
    }
}
