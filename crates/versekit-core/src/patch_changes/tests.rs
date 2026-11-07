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

use std::sync::Mutex;

use super::*;
use crate::erkul_fetch::{FnFeed, pack};
use crate::paths::Host;
use crate::pypath::Flavor;

const CATALOG: &str = r#"{"patches": [
    {"dataVersion": "4.9.0-LIVE.12232306", "generatedAt": "2026-08-01T10:00:00Z",
     "summary": {"added": 24, "removed": 4, "modified": 250, "unchanged": 9000}, "path": "changelog.49.bin"},
    {"dataVersion": "4.10.0-LIVE.12519617", "generatedAt": "2026-10-02T10:00:00Z",
     "summary": {"added": 17, "removed": 1, "modified": 352}, "path": "changelog.410.bin"},
    {"dataVersion": "4.10.1-LIVE.12600000", "generatedAt": "2026-10-09T10:00:00Z",
     "summary": {"added": 0, "removed": 0, "modified": 0, "unchanged": 9100}, "path": "changelog.4101.bin"},
    {"dataVersion": "4.10.2-LIVE.12700000", "generatedAt": "2026-10-16T10:00:00Z",
     "summary": {"modified": 3}, "path": "changelog.missing.bin"}]}"#;

const CHANGELOG: &str = r#"{"summary": {"added": 1, "removed": 1, "modified": 2}, "categories": [
    {"kind": "weapons", "unchanged": 400, "added": [{"id": "w1", "name": "New Gun", "size": 3, "changes": []}],
     "modified": [{"id": "w2", "className": "C788_Cannon", "changes": [
        {"path": "weapon.ammo.impactRadius", "oldValue": 1090},
        {"path": "weapon.ammo.speed", "oldValue": 110, "newValue": 120}]}]},
    {"kind": "ships", "removed": [{"id": "s1", "name": "Old Ship"}],
     "modified": [{"id": "s2", "name": "Cutter", "changes": [
        {"path": "precomputed.fuel.burnRate.main", "oldValue": 1.25e-06, "newValue": 2.67e-07}]},
        {"id": "s3", "name": "Aurora", "changes": [{"path": "x", "newValue": 5}]}]}]}"#;

fn feed(asked: &'static Mutex<Vec<String>>, with_net: bool) -> impl Feed {
    FnFeed(move |path: &str| {
        asked.lock().unwrap().push(path.to_string());
        if !with_net {
            return None;
        }
        match path {
            "LIVE/catalog.bin" => Some(pack(CATALOG)),
            "LIVE/changelog.49.bin" | "LIVE/changelog.410.bin" => Some(pack(CHANGELOG)),
            _ => None,
        }
    })
}

fn paths_in(name: &str) -> (Paths, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!("versekit-patches-{}-{name}", crate::testdir::run_tag()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let home = dir.to_string_lossy().into_owned();
    (Paths::new(Host::new(Flavor::native(), &[(crate::legacy::ENV_HOME, home.as_str())], &home)), dir)
}

#[test]
fn only_patches_with_changes_are_fetched_and_kept_once() {
    static ASKED: Mutex<Vec<String>> = Mutex::new(Vec::new());
    let (paths, dir) = paths_in("sync");
    let new = sync(&paths, &feed(&ASKED, true)).expect("reachable");
    assert_eq!(
        new,
        ["4.10.0-LIVE.12519617", "4.9.0-LIVE.12232306"],
        "newest first, the empty one and the missing one not"
    );
    assert!(!ASKED.lock().unwrap().iter().any(|p| p.contains("4101")), "an empty patch is never fetched");
    assert_eq!(stored(&paths), ["4.10.0-LIVE.12519617", "4.9.0-LIVE.12232306"]);
    let kept = load(&paths, "4.10.0-LIVE.12519617").expect("kept");
    assert_eq!(text(&kept, "quelle"), "erkul.games");
    assert_eq!(text(&kept, "datum"), "2026-10-02");
    let raw = std::fs::read_to_string(file(&paths, "4.10.0-LIVE.12519617")).unwrap();
    assert!(!raw.contains(", ") && !raw.contains(": "), "compact");
    ASKED.lock().unwrap().clear();
    assert_eq!(sync(&paths, &feed(&ASKED, true)), Ok(Vec::new()), "nothing new");
    assert!(ASKED.lock().unwrap().iter().all(|p| !p.contains("changelog.4")), "kept ones are not fetched again");
    assert_eq!(sync(&paths, &feed(&ASKED, false)), Err(Unreachable::Source));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_collection_keeps_what_the_source_dropped() {
    static ASKED: Mutex<Vec<String>> = Mutex::new(Vec::new());
    let (paths, dir) = paths_in("overview");
    sync(&paths, &feed(&ASKED, true)).unwrap();
    // The source now offers only the newest two.
    let offer: Vec<Offered> = offered(&feed(&ASKED, true)).unwrap().into_iter().take(2).collect();
    let list = overview(&paths, &offer);
    let seen: Vec<(&str, bool, bool, bool)> =
        list.iter().map(|p| (p.short.as_str(), p.empty, p.at_source, p.kept)).collect();
    assert_eq!(
        seen,
        [
            ("4.10.2", false, true, false),
            ("4.10.1", true, true, false),
            ("4.10.0", false, false, true),
            ("4.9.0", false, false, true),
        ],
        "a patch the source dropped stays, kept here"
    );
    assert_eq!(list[2].summary, Summary { added: 1, removed: 1, modified: 2 }, "the summary of the file");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_changes_are_flat_with_missing_values_kept_apart() {
    static ASKED: Mutex<Vec<String>> = Mutex::new(Vec::new());
    let (paths, dir) = paths_in("changes");
    sync(&paths, &feed(&ASKED, true)).unwrap();
    let all = changes(&paths, "4.10.0-LIVE.12519617", None);
    let seen: Vec<(&str, Fate, &str)> = all.iter().map(|c| (c.kind.as_str(), c.fate, c.name.as_str())).collect();
    assert_eq!(
        seen,
        [
            ("weapons", Fate::New, "New Gun"),
            ("weapons", Fate::Changed, "C788_Cannon"),
            ("ships", Fate::Gone, "Old Ship"),
            ("ships", Fate::Changed, "Cutter"),
            ("ships", Fate::Changed, "Aurora"),
        ]
    );
    assert_eq!(categories(&all), [("ships".to_string(), 3), ("weapons".to_string(), 2)], "the largest first");
    assert_eq!(changes(&paths, "4.10.0-LIVE.12519617", Some("weapons")).len(), 2);
    let cannon = &all[1].fields;
    assert_eq!((cannon[0].old.is_some(), cannon[0].new.is_none()), (true, true), "a field that went");
    assert_eq!(all[0].size, Some(Value::int(3)));
    assert!(changes(&paths, "4.10.1-LIVE.12600000", None).is_empty(), "not kept");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn values_are_written_as_the_python_version_writes_them() {
    crate::language::set_language(crate::language::Choice::De);
    let f = |x: f64| number_text(Some(&Value::Float(x)), 4);
    assert_eq!(f(975.0), "975");
    assert_eq!(f(0.30000000004), "0.3");
    assert_eq!(f(123_456.789), "123457", "%g: six digits");
    assert_eq!(f(1_234_567.0), "1.23457e+06");
    assert_eq!(f(0.00012), "0.0001", "rounded to four places first");
    assert_eq!(number_text(Some(&Value::Float(0.0000125)), 8), "0.0000125", "small ones written out");
    assert_eq!(number_text(Some(&Value::Bool(true)), 4), "ja");
    assert_eq!(number_text(Some(&Value::int(42)), 4), "42");
    assert_eq!(number_text(None, 4), "—");
    assert_eq!(number_text(Some(&jsonio::loads("[1]").unwrap()), 4), "…");
}

#[test]
fn a_difference_always_shows_and_never_as_a_false_zero() {
    let v = |x: f64| Value::Float(x);
    // Four places: both 0; six: the second a false 0; eight: both shown.
    assert_eq!(pair(Some(&v(1.25e-06)), Some(&v(2.67e-07))), ("0.00000125".into(), "0.00000027".into()));
    assert_eq!(pair(Some(&Value::int(110)), Some(&Value::int(120))), ("110".into(), "120".into()));
    assert_eq!(percent(Some(&Value::int(110)), Some(&Value::int(120))), "  (+9 %)");
    assert_eq!(percent(Some(&Value::int(100)), Some(&Value::int(350))), "  (×3.5)");
    assert_eq!(percent(Some(&Value::int(0)), Some(&Value::int(5))), "", "no base");
    assert_eq!(percent(Some(&Value::int(1000)), Some(&Value::int(1004))), "", "under half a percent");
    assert_eq!(percent(Some(&Value::Bool(false)), Some(&Value::Bool(true))), "");
    assert_eq!(direction(Some(&Value::int(1)), Some(&Value::int(2))), 1);
    assert_eq!(direction(Some(&Value::int(2)), Some(&Value::int(1))), -1);
    assert_eq!(direction(Some(&Value::Bool(false)), Some(&Value::Bool(true))), 0, "truth is no rise");
}

#[test]
fn nested_values_name_what_differs() {
    crate::language::set_language(crate::language::Choice::De);
    let old = jsonio::loads(r#"[{"consumes": [{"resource": "Power", "units": 4}], "gone": 1}]"#).unwrap();
    let new = jsonio::loads(r#"[{"consumes": [{"resource": "Power", "units": 6}], "added": 2}]"#).unwrap();
    // In the order of the full paths: `[0].added`, `[0].consumes[0].units`, `[0].gone`.
    assert_eq!(struct_diff(&old, &new, 3), "added neu: 2, units 4 → 6, gone fällt weg");
    assert_eq!(struct_diff(&old, &old, 3), t("s_pa_gleich"));
    assert!(struct_diff(&old, &new, 1).ends_with(&tf("s_pa_mehr", &[("n", Arg::from(2))])));
    let change = |old: Option<Value>, new: Option<Value>| FieldChange { path: "p".into(), old, new };
    assert_eq!(field_text(&change(Some(Value::int(1090)), None)), ("1090 → fällt weg".to_string(), Tone::Gone));
    assert_eq!(field_text(&change(None, Some(Value::int(5)))), ("neu: 5".to_string(), Tone::Up));
    assert_eq!(field_text(&change(Some(Value::int(3)), Some(Value::int(3)))), ("3 → 3".to_string(), Tone::Same));
    assert_eq!(field_text(&change(Some(Value::int(4)), Some(Value::int(2)))).1, Tone::Down);
}

#[test]
fn a_version_from_the_net_stays_in_its_folder() {
    assert_eq!(safe_name("../../evil/4.10"), ".._.._evil_4.10");
    assert_eq!(safe_name(""), "unbekannt");
    let dir = std::env::temp_dir().join(format!("versekit-patches-{}-ordner", crate::testdir::run_tag()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let appdata = dir.to_string_lossy().into_owned();
    let vars = [
        ("APPDATA", appdata.as_str()),
        ("LOCALAPPDATA", appdata.as_str()),
        ("USERPROFILE", appdata.as_str()),
        ("HOME", appdata.as_str()),
        ("XDG_CONFIG_HOME", appdata.as_str()),
    ];
    let home = Host::new(Flavor::native(), &vars, &appdata);
    let paths = Paths::new(home);
    let target = folder(&paths);
    assert!(target.ends_with(FOLDER) && target.starts_with(&appdata), "{target}");
    let _ = std::fs::remove_dir_all(&dir);
}
