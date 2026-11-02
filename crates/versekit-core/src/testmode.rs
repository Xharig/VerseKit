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

//! The test mode: a run on a copy of the data, started with `--daten <folder>`.
//!
//! It is the run whose writing `writeguard` locks to the copy, and it is told
//! by the same source: [`Paths::for_copy`] marks the data folder as given by
//! the command line ([`GivenBy::CommandLine`]), and only it does. A copy holds
//! the credentials and the installation of the real data, so what leaves the
//! computer from it would count as the player's own:
//!
//! * the Basetool is locked: no request leaves, no key is opened or deleted
//!   ([`basetool_locked`]); only [`ENV_BASETOOL_IN_TEST`] set to `1` opens it
//!   for a test that wants it; opened, the key of the real installation is
//!   still never deleted ([`basetool_keeps_key`]);
//! * the daily usage report goes to a receiver on this computer only
//!   ([`usage_allowed`]), never to the real one.

use crate::given::{Fixed, GivenBy};
use crate::paths::{Host, Paths};

/// Set to `1`, the Basetool is reachable in the test mode as well.
pub const ENV_BASETOOL_IN_TEST: &str = "VERSEKIT_BASETOOL_IM_TEST";

/// The most changes this run of the program may send to the Basetool, all sends together (a whole number;
/// `0` only fetches). Not set, the program sends as before on the player's own data, and a copy opened for a
/// test only fetches ([`basetool_write_ceiling`]).
pub const ENV_BASETOOL_WRITE_MAX: &str = "VERSEKIT_BASETOOL_WRITE_MAX";

/// The German name of [`ENV_BASETOOL_WRITE_MAX`], as the plan of the test wrote it. Both count; set both,
/// the smaller ceiling holds.
pub const ENV_BASETOOL_WRITE_MAX_DE: &str = "VERSEKIT_BASETOOL_SCHREIBEN_MAX";

/// Set to `1`, every request to the Basetool is noted in `basetool-anfragen.log` (no content).
pub const ENV_BASETOOL_JOURNAL: &str = "VERSEKIT_BASETOOL_JOURNAL";

/// The code of a request the lock refused (`ApiError::code`).
pub const BASETOOL_LOCKED: &str = "TEST_MODE_LOCKED";

/// The code of a send the write guard refused (`ApiError::code`).
pub const WRITE_GUARD: &str = "WRITE_GUARD";

/// Whether these paths are those of a copy of the data (`--daten`).
pub fn on_copy(paths: &Paths) -> bool {
    matches!(paths.given().data, Some(Fixed { by: GivenBy::CommandLine, .. }))
}

/// Whether the Basetool is opened for a test: [`ENV_BASETOOL_IN_TEST`] is `1`.
pub fn basetool_opened(host: &Host) -> bool {
    host.var(ENV_BASETOOL_IN_TEST).is_some_and(|value| crate::text::py_strip(value) == "1")
}

/// Whether nothing may reach the Basetool: a copy of the data, not opened for a test.
pub fn basetool_locked(paths: &Paths) -> bool {
    on_copy(paths) && !basetool_opened(&paths.host)
}

/// The ceiling the variables set: `None` while neither [`ENV_BASETOOL_WRITE_MAX`] nor
/// [`ENV_BASETOOL_WRITE_MAX_DE`] is set. Set, only a plain whole number counts; anything else (empty, `-1`,
/// `viele`, a number too large) is `0`, the strictest, so a typo never loosens the guard. Both set, the smaller
/// holds.
pub fn basetool_write_max(host: &Host) -> Option<u32> {
    let ceiling = |value: &str| {
        let text = crate::text::py_strip(value);
        let digits = !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit());
        if digits { text.parse().unwrap_or(0) } else { 0 }
    };
    [ENV_BASETOOL_WRITE_MAX, ENV_BASETOOL_WRITE_MAX_DE].into_iter().filter_map(|name| host.var(name)).map(ceiling).min()
}

/// The ceiling of the write guard for a run on `paths`: the one of the variables ([`basetool_write_max`]); on a
/// copy of the data opened for a test without them `0`, so a lost or misspelt variable only fetches. On the
/// player's own data without the variables `None`: the program sends as before.
pub fn basetool_write_ceiling(paths: &Paths) -> Option<u32> {
    basetool_write_max(&paths.host).or_else(|| (on_copy(paths) && basetool_opened(&paths.host)).then_some(0))
}

/// The name a copy of the data gives its installation in the Basetool while none is entered: never the one of
/// the player's own program, so the two are told apart in the Basetool's list of installations.
pub const BASETOOL_TEST_LABEL: &str = "VerseKit Echttest";

/// The name of the installation while none is entered: [`BASETOOL_TEST_LABEL`] on a copy of the data, else the
/// one of the system (`config::default_label`).
pub fn basetool_default_label(paths: &Paths) -> &'static str {
    if on_copy(paths) { BASETOOL_TEST_LABEL } else { crate::basetool::config::default_label() }
}

/// Whether "Disconnect" must leave the key of the installation alone: a copy of the data whose key carries the
/// name of the player's real installation (no `SC_BP_BASETOOL_KEYNAME` of its own). Deleting it would cut the
/// player's own program off the Basetool.
pub fn basetool_keeps_key(paths: &Paths) -> bool {
    on_copy(paths) && !crate::basetool::key::own_key_name(&paths.host)
}

/// Whether the request journal is on: [`ENV_BASETOOL_JOURNAL`] is `1`.
pub fn basetool_journal_on(host: &Host) -> bool {
    host.var(ENV_BASETOOL_JOURNAL).is_some_and(|value| crate::text::py_strip(value) == "1")
}

/// Whether the usage report may go to `address`: always on the player's own data, on a copy only to a
/// receiver on this computer (`http://127.0.0.1`, a made-up one of the checks).
pub fn usage_allowed(paths: &Paths, address: &str) -> bool {
    !on_copy(paths) || crate::report_send::loopback_http(address)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pypath::Flavor;

    /// A copy of the data in a throwaway folder, as `--daten` makes it, with `vars` on its host.
    fn copy(name: &str, vars: &[(&str, &str)]) -> (Paths, std::path::PathBuf) {
        let root = crate::testdir::fresh(&format!("testbetrieb-{name}"));
        std::fs::create_dir_all(root.join(crate::paths::COPY_DATA_FOLDER)).unwrap();
        let text = root.to_string_lossy().into_owned();
        let host = Host::new(Flavor::native(), vars, &text);
        (Paths::for_copy(host, &text).expect("a copy"), root)
    }

    #[test]
    fn a_copy_of_the_data_is_the_test_mode_and_nothing_else_is() {
        let (paths, root) = copy("kopie", &[]);
        assert!(on_copy(&paths) && basetool_locked(&paths));
        // Counter-checks: the player's own data, a data folder named by the variable of the checks, and one
        // another part of the program fixed are no copy.
        let dir = crate::testdir::Dir::new("testbetrieb-eigen");
        let text = dir.path().to_string_lossy().into_owned();
        let own = Paths::new(Host::new(Flavor::native(), &[], &text));
        let named = Paths::new(Host::new(Flavor::native(), &[(crate::legacy::ENV_HOME, &text)], &text));
        let fixed = Paths::new(Host::new(Flavor::native(), &[], &text));
        fixed.set_given_data(&text, GivenBy::Named("Test".into()));
        for (what, paths) in [("own", &own), ("variable", &named), ("named", &fixed)] {
            assert!(!on_copy(paths) && !basetool_locked(paths), "{what}");
        }
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn only_the_variable_set_to_one_opens_the_basetool_on_a_copy() {
        let (opened, root) = copy("offen", &[(ENV_BASETOOL_IN_TEST, " 1 ")]);
        assert!(on_copy(&opened) && !basetool_locked(&opened));
        let _ = std::fs::remove_dir_all(root);
        for value in ["", "0", "ja", "true", "11"] {
            let (paths, root) = copy("zu", &[(ENV_BASETOOL_IN_TEST, value)]);
            assert!(basetool_locked(&paths), "{value:?} keeps it locked");
            let _ = std::fs::remove_dir_all(root);
        }
    }

    #[test]
    fn the_ceiling_of_the_write_guard_is_a_plain_number_and_anything_else_is_zero() {
        let host = |vars: &[(&str, &str)]| Host::new(Flavor::native(), vars, "/");
        assert_eq!(basetool_write_max(&host(&[])), None, "not set: no guard");
        for (value, wanted) in [("0", 0), ("1", 1), (" 25 ", 25), ("007", 7)] {
            assert_eq!(basetool_write_max(&host(&[(ENV_BASETOOL_WRITE_MAX, value)])), Some(wanted), "{value:?}");
        }
        // A value that is no whole number never loosens the guard.
        for value in ["", " ", "-1", "+1", "1.5", "viele", "1 2", "0x10"] {
            assert_eq!(basetool_write_max(&host(&[(ENV_BASETOOL_WRITE_MAX, value)])), Some(0), "{value:?}");
        }
        // The German name counts as well, and of both the smaller holds: a name from the plan never runs unguarded.
        assert_eq!(basetool_write_max(&host(&[(ENV_BASETOOL_WRITE_MAX_DE, "1")])), Some(1));
        assert_eq!(
            basetool_write_max(&host(&[(ENV_BASETOOL_WRITE_MAX, "5"), (ENV_BASETOOL_WRITE_MAX_DE, "1")])),
            Some(1)
        );
        assert_eq!(
            basetool_write_max(&host(&[(ENV_BASETOOL_WRITE_MAX, "0"), (ENV_BASETOOL_WRITE_MAX_DE, "9")])),
            Some(0)
        );
        assert_eq!(basetool_write_max(&host(&[(ENV_BASETOOL_WRITE_MAX_DE, "abc")])), Some(0));
        // A number beyond what the guard can count is a typo too.
        for value in ["4294967296", "99999999999999999999"] {
            assert_eq!(basetool_write_max(&host(&[(ENV_BASETOOL_WRITE_MAX, value)])), Some(0), "{value:?}");
        }
        assert_eq!(basetool_write_max(&host(&[(ENV_BASETOOL_WRITE_MAX, "4294967295")])), Some(u32::MAX));
    }

    #[test]
    fn a_copy_opened_for_a_test_without_the_variable_only_fetches() {
        let (opened, root) = copy("deckel-fehlt", &[(ENV_BASETOOL_IN_TEST, "1")]);
        assert_eq!(basetool_write_ceiling(&opened), Some(0), "a lost variable never runs unguarded");
        let _ = std::fs::remove_dir_all(root);
        let (given, root) = copy("deckel-da", &[(ENV_BASETOOL_IN_TEST, "1"), (ENV_BASETOOL_WRITE_MAX_DE, "1")]);
        assert_eq!(basetool_write_ceiling(&given), Some(1), "the variable holds");
        let _ = std::fs::remove_dir_all(root);
        // A locked copy sends nothing anyway, and the player's own data send as before.
        let (locked, root) = copy("deckel-gesperrt", &[]);
        assert_eq!(basetool_write_ceiling(&locked), None);
        let _ = std::fs::remove_dir_all(root);
        let own = Paths::new(Host::new(Flavor::native(), &[(ENV_BASETOOL_IN_TEST, "1")], "/"));
        assert_eq!(basetool_write_ceiling(&own), None, "own data: no guard without the variable");
        let own_given = Paths::new(Host::new(Flavor::native(), &[(ENV_BASETOOL_WRITE_MAX, "2")], "/"));
        assert_eq!(basetool_write_ceiling(&own_given), Some(2));
    }

    #[test]
    fn on_a_copy_with_the_real_key_name_the_key_is_kept() {
        use crate::legacy::ENV_BASETOOL_KEYNAME;
        let (shared, root) = copy("schluessel-geteilt", &[(ENV_BASETOOL_IN_TEST, "1")]);
        assert!(basetool_keeps_key(&shared), "the key of the real installation stays");
        let _ = std::fs::remove_dir_all(root);
        // The real name given by hand is still the real key.
        let (named_real, root) = copy("schluessel-echt", &[(ENV_BASETOOL_KEYNAME, crate::basetool::key::KEY_NAME)]);
        assert!(basetool_keeps_key(&named_real));
        let _ = std::fs::remove_dir_all(root);
        // Counter-checks: a key of its own on the copy, and the player's own data, may be deleted.
        let (own_key, root) = copy("schluessel-eigen", &[(ENV_BASETOOL_KEYNAME, "VerseKit Echttest")]);
        assert!(!basetool_keeps_key(&own_key));
        let _ = std::fs::remove_dir_all(root);
        let own = Paths::new(Host::new(Flavor::native(), &[], "/"));
        assert!(!basetool_keeps_key(&own));
    }

    #[test]
    fn a_copy_names_its_installation_apart_from_the_players_own() {
        let (paths, root) = copy("name", &[(ENV_BASETOOL_IN_TEST, "1")]);
        assert_eq!(basetool_default_label(&paths), "VerseKit Echttest");
        assert!(crate::basetool::config::label_valid(BASETOOL_TEST_LABEL));
        let _ = std::fs::remove_dir_all(root);
        let own = Paths::new(Host::new(Flavor::native(), &[], "/"));
        assert_eq!(basetool_default_label(&own), crate::basetool::config::default_label(), "own data unchanged");
    }

    #[test]
    fn the_journal_is_on_only_for_a_one() {
        let host = |vars: &[(&str, &str)]| Host::new(Flavor::native(), vars, "/");
        assert!(basetool_journal_on(&host(&[(ENV_BASETOOL_JOURNAL, " 1 ")])));
        for value in ["", "0", "ja", "true", "11"] {
            assert!(!basetool_journal_on(&host(&[(ENV_BASETOOL_JOURNAL, value)])), "{value:?}");
        }
        assert!(!basetool_journal_on(&host(&[])));
    }

    #[test]
    fn on_a_copy_the_usage_report_goes_to_this_computer_only() {
        let (paths, root) = copy("nutzung", &[]);
        assert!(!usage_allowed(&paths, crate::usage::TARGET));
        assert!(usage_allowed(&paths, "http://127.0.0.1:9/ping"));
        let _ = std::fs::remove_dir_all(root);
        let own = Paths::new(Host::new(Flavor::native(), &[], "/"));
        assert!(usage_allowed(&own, crate::usage::TARGET), "the player's own data report as before");
    }
}
