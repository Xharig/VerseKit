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

use super::*;
use crate::testdir::Dir;

const A: &str = "https://a.example.org";
const B: &str = "https://b.example.org";

fn paths_in(name: &str) -> (Dir, Paths) {
    let folder = Dir::new(name);
    let text = folder.path().to_string_lossy().into_owned();
    let host = crate::paths::Host::new(crate::pypath::Flavor::Posix, &[(crate::envvars::ENV_HOME, &text)], &text);
    (folder, Paths::new(host))
}

fn connection(id: &str, scopes: &[&str]) -> ConnectionView {
    ConnectionView {
        id: id.into(),
        name: "Orga".into(),
        tag: "ORG".into(),
        member: "Pilot".into(),
        scopes: scopes.iter().map(|s| s.to_string()).collect(),
        connected_at: 1,
        checked_at: None,
    }
}

fn view(connections: Vec<ConnectionView>) -> View {
    View { connections, ..View::default() }
}

#[test]
fn a_fresh_connection_shows_nothing_whatever_it_may() {
    let areas = Areas::in_memory();
    let all = view(vec![connection(A, &crate::orgstd::SCOPES)]);
    for kind in Kind::ALL {
        assert!(!all.area_visible(&areas, None, kind), "{kind:?} shows without a switch");
        assert!(!areas.on(A, kind));
    }
}

#[test]
fn an_area_shows_only_with_the_switch_the_right_and_the_connection() {
    let areas = Areas::in_memory();
    assert!(areas.set(A, Kind::Stock, true));
    assert!(!view(Vec::new()).area_visible(&areas, None, Kind::Stock), "switch on, but not connected");
    let without_right = view(vec![connection(A, &["orders:read"])]);
    assert!(!without_right.area_visible(&areas, None, Kind::Stock), "switch on, but the right is missing");
    let right = view(vec![connection(A, &["stock:read"])]);
    assert!(right.area_visible(&areas, None, Kind::Stock));
    assert!(!right.area_visible(&areas, None, Kind::Blueprints), "the switch of another area is off");
    assert!(!right.area_visible(&areas, None, Kind::Orders));
    assert!(areas.set(A, Kind::Stock, false));
    assert!(!right.area_visible(&areas, None, Kind::Stock), "switched off again");
}

#[test]
fn the_switches_count_per_org() {
    let areas = Areas::in_memory();
    areas.set(A, Kind::Stock, true);
    let both = view(vec![connection(A, &["stock:read"]), connection(B, &["stock:read"])]);
    assert!(both.area_visible(&areas, Some(A), Kind::Stock));
    assert!(!both.area_visible(&areas, Some(B), Kind::Stock));
    assert!(both.area_visible(&areas, None, Kind::Stock), "any org is enough when none is named");
    let shown: Vec<&str> = both.orgs_with(&areas, Kind::Stock).map(|c| c.id.as_str()).collect();
    assert_eq!(shown, [A]);
}

#[test]
fn the_switches_are_kept_in_the_settings_and_disconnecting_removes_the_entry() {
    let (folder, paths) = paths_in("orga-bereiche");
    let areas = Areas::in_settings(paths);
    assert!(!areas.on(A, Kind::Stock));
    assert!(
        areas.set(A, Kind::Stock, true) && areas.set(B, Kind::Orders, true) && areas.set(A, Kind::Blueprints, false)
    );
    let text = std::fs::read_to_string(folder.path().join("einstellungen.json")).unwrap();
    assert!(text.contains("orga_bereiche") && text.contains("\"lager\": true"), "{text}");
    // Another start reads the same.
    let (_other, again) = (&folder, paths_at(&folder));
    let reread = Areas::in_settings(again);
    assert!(reread.on(A, Kind::Stock) && reread.on(B, Kind::Orders));
    assert!(!reread.on(A, Kind::Blueprints) && !reread.on(B, Kind::Stock));
    assert!(reread.forget(A));
    assert!(!reread.on(A, Kind::Stock) && reread.on(B, Kind::Orders), "only the entry of the org goes");
    let text = std::fs::read_to_string(folder.path().join("einstellungen.json")).unwrap();
    assert!(!text.contains(A) && text.contains(B), "{text}");
}

fn paths_at(folder: &Dir) -> Paths {
    let text = folder.path().to_string_lossy().into_owned();
    Paths::new(crate::paths::Host::new(crate::pypath::Flavor::Posix, &[(crate::envvars::ENV_HOME, &text)], &text))
}

#[test]
fn a_damaged_setting_counts_as_off_and_is_repaired_by_the_next_switch() {
    let (_folder, paths) = paths_in("orga-bereiche-kaputt");
    assert!(paths.set_setting(SETTING, Value::str("kaputt")));
    let areas = Areas::in_settings(paths);
    assert!(!areas.on(A, Kind::Stock));
    assert!(areas.set(A, Kind::Stock, true));
    assert!(areas.on(A, Kind::Stock));
}
