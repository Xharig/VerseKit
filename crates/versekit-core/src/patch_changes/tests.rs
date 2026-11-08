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
    crate::language::set_thread_language(crate::language::Choice::De);
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
    crate::language::set_thread_language(crate::language::Choice::De);
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

#[test]
fn a_kept_patch_says_how_many_items_stayed_the_same() {
    static ASKED: Mutex<Vec<String>> = Mutex::new(Vec::new());
    let (paths, dir) = paths_in("unveraendert");
    sync(&paths, &feed(&ASKED, true)).unwrap();
    assert_eq!(unchanged(&paths, "4.10.0-LIVE.12519617"), None, "one of two areas says it: no part as the whole");
    assert_eq!(unchanged(&paths, "4.10.1-LIVE.12600000"), None, "not kept");
    // A file whose summary says it wins over the areas.
    let target = file(&paths, "4.11.0-LIVE.1");
    let write = |text: &str| std::fs::write(&target, text).unwrap();
    write(
        r#"{"version": "4.11.0-LIVE.1", "summary": {"unchanged": 9000},
            "categories": [{"kind": "ships", "unchanged": 5}]}"#,
    );
    assert_eq!(unchanged(&paths, "4.11.0-LIVE.1"), Some(9000));
    let areas = r#"[{"kind": "ships", "unchanged": 5}, {"kind": "weapons", "unchanged": 7}]"#;
    write(&format!(r#"{{"version": "4.11.0-LIVE.1", "summary": {{"unchanged": null}}, "categories": {areas}}}"#));
    assert_eq!(unchanged(&paths, "4.11.0-LIVE.1"), Some(12), "an empty summary leaves it to the areas, all of them");
    write(r#"{"version": "4.11.0-LIVE.1", "categories": [{"kind": "ships"}]}"#);
    assert_eq!(unchanged(&paths, "4.11.0-LIVE.1"), None, "nothing said, nothing counted");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_largest_rises_and_falls_are_found_in_percent_of_the_old_value() {
    static ASKED: Mutex<Vec<String>> = Mutex::new(Vec::new());
    let (paths, dir) = paths_in("groesste");
    sync(&paths, &feed(&ASKED, true)).unwrap();
    let all = changes(&paths, "4.10.0-LIVE.12519617", None);
    let (rises, falls) = largest_moves(&all, 3);
    let named = |list: &[Move]| list.iter().map(|m| (m.name.clone(), m.path.clone())).collect::<Vec<_>>();
    assert_eq!(named(&rises), [("C788_Cannon".to_string(), "weapon.ammo.speed".to_string())]);
    assert_eq!(named(&falls), [("Cutter".to_string(), "precomputed.fuel.burnRate.main".to_string())]);
    assert!((rises[0].share - 100.0 / 11.0).abs() < 1e-9, "{}", rises[0].share);
    assert!((falls[0].share + 78.64).abs() < 1e-9, "{}", falls[0].share);
    let _ = std::fs::remove_dir_all(&dir);

    let field = |path: &str, old: Value, new: Value| FieldChange { path: path.into(), old: Some(old), new: Some(new) };
    let item = |fate: Fate, name: &str, fields: Vec<FieldChange>| Change {
        kind: "weapons".into(),
        fate,
        id: String::new(),
        name: name.into(),
        size: None,
        fields,
    };
    let (f, i) = (Value::Float, Value::int);
    let made = [
        item(
            Fate::Changed,
            "A",
            vec![
                field("up.small", i(100), i(110)),
                field("up.big", f(2.0), f(7.0)),
                field("from.zero", i(0), i(5)),
                field("truth", Value::Bool(false), Value::Bool(true)),
                field("tiny", i(1000), i(1004)),
                field("down", i(100), i(40)),
                field("negative.base", f(-1.0), f(2.0)),
                field("endless", f(1e-300), f(1e300)),
            ],
        ),
        item(Fate::Changed, "B", vec![field("up.same", i(100), i(110)), field("down.more", f(4.0), f(0.5))]),
        // A new item has no old value to compare with, even when its file names one.
        item(Fate::New, "C", vec![field("up.huge", i(1), i(1000))]),
    ];
    let (rises, falls) = largest_moves(&made, 2);
    let paths_of = |list: &[Move]| list.iter().map(|m| m.path.as_str()).collect::<Vec<_>>().join(",");
    assert_eq!(paths_of(&rises), "up.big,up.small", "the largest first, cut at two, equal ones in their order");
    assert_eq!(paths_of(&falls), "down.more,down", "by size: -87.5 % before -60 %");
    let rises = largest_moves(&made, 9).0;
    // Left out: a base of zero or below, truth, a tiny change, an endless share, a new item.
    assert_eq!(paths_of(&rises), "up.big,up.small,up.same");
    assert_eq!(share_text(Some(&i(100)), Some(&i(120))), "+20 %");
    assert_eq!(share_text(Some(&i(100)), Some(&i(350))), "×3.5");
    assert_eq!(share_text(Some(&i(1000)), Some(&i(1004))), "");
}

#[test]
fn the_patches_looked_at_are_kept_and_the_others_are_unread() {
    let (paths, dir) = paths_in("gelesen");
    assert_eq!(seen(&paths), None, "never opened");
    let patch = |version: &str, empty: bool| Patch {
        version: version.into(),
        short: String::new(),
        date: String::new(),
        summary: Summary::default(),
        empty,
        at_source: true,
        kept: false,
    };
    let list = [patch("4.11.0-LIVE.2", false), patch("4.10.1-LIVE.1", true), patch("4.10.0-LIVE.1", false)];
    let looked: BTreeSet<String> = ["4.10.0-LIVE.1".to_string()].into();
    set_seen(&paths, &looked).unwrap();
    assert_eq!(seen(&paths), Some(looked.clone()));
    assert!(stored(&paths).is_empty(), "the file is no patch file");
    assert_eq!(unread(&list, &looked), ["4.11.0-LIVE.2"], "a patch without changes is never unread");
    assert!(unread(&list, &list.iter().map(|p| p.version.clone()).collect()).is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}
