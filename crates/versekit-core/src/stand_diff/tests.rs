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
use crate::paths::Host;
use crate::pypath::Flavor;
use crate::stands::{PROBE_PLACEHOLDERS, probe_of};

fn paths_in(name: &str) -> (Paths, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!("versekit-stand-diff-{}-{name}", crate::testdir::run_tag()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let home = dir.to_string_lossy().into_owned();
    (Paths::new(Host::new(Flavor::native(), &[(crate::envvars::ENV_HOME, home.as_str())], &home)), dir)
}

fn text(s: &str) -> Leaf {
    Leaf::Text(s.to_string())
}

fn entry(values: &[(&str, Leaf)]) -> Entry {
    values.iter().map(|(path, leaf)| ((*path).to_string(), leaf.clone())).collect()
}

/// A stand of the reader `reader` with the given areas, each a list of `(key, values)`.
fn stand_of(reader: &str, areas: &[(&str, Vec<(&str, Entry)>)]) -> Stand {
    let mut stand = Stand::new("LIVE", "4.10.0-LIVE.12345678", "2026-01-01", reader);
    for (name, entries) in areas {
        let mut area = Area::default();
        for (key, entry) in entries {
            area.entries.insert((*key).to_string(), entry.clone());
        }
        stand.areas.insert((*name).to_string(), area);
    }
    stand.probe = probe_of(&stand.areas);
    stand
}

fn changes_of(cmp: &Comparison) -> Vec<(String, Option<Leaf>, Option<Leaf>, Kind)> {
    cmp.rows
        .iter()
        .flat_map(|r| r.changes.iter().map(|c| (c.path.clone(), c.old.clone(), c.new.clone(), c.kind)))
        .collect()
}

#[test]
fn the_same_stand_compared_with_itself_is_empty() {
    let stand = stand_of(
        "r",
        &[
            ("katalog.bauplaene", vec![("a", entry(&[("name", text("Alpha")), ("rate", Leaf::number(0.1 + 0.2))]))]),
            ("rufstufen.stufen", vec![("r1", entry(&[("", Leaf::Int(0))])), ("r2", entry(&[("", Leaf::Int(100))]))]),
        ],
    );
    let cmp = compare(&stand, &stand.clone());
    assert!(cmp.is_empty(), "{cmp:?}");
    assert_eq!(cmp.summary(), (0, 0, 0));
    assert_eq!(cmp.counts.iter().map(|c| c.unchanged).sum::<usize>(), 3, "all three entries are the same");
    assert_eq!(cmp.classes(), Classes::default());
}

#[test]
fn every_kind_of_change_is_found_once() {
    let old = stand_of(
        "r",
        &[(
            "teile.teile",
            vec![
                ("a", entry(&[("hp", Leaf::Int(10)), ("name", text("Alpha"))])),
                ("b", entry(&[("hp", Leaf::Int(1))])),
            ],
        )],
    );
    let new = stand_of(
        "r",
        &[(
            "teile.teile",
            vec![
                ("a", entry(&[("hp", Leaf::Int(12)), ("name", text("Alpha 2")), ("extra", Leaf::Int(1))])),
                ("c", entry(&[("hp", Leaf::Int(5))])),
            ],
        )],
    );
    let cmp = compare(&old, &new);
    let kinds: Vec<Kind> = changes_of(&cmp).into_iter().map(|c| c.3).collect();
    assert_eq!(kinds, [Kind::Shape, Kind::Value, Kind::Text], "a field that came, a number, a text, by path");
    let rows: Vec<(Fate, &str)> = cmp.rows.iter().map(|r| (r.fate, r.id.as_str())).collect();
    assert_eq!(rows, [(Fate::Changed, "a"), (Fate::New, "c"), (Fate::Gone, "b")]);
    assert_eq!(cmp.summary(), (1, 1, 1));
    assert_eq!(cmp.rows.iter().map(Row::kind).collect::<Vec<_>>(), [Kind::Shape, Kind::New, Kind::Gone]);
}

#[test]
fn a_number_becoming_a_range_is_a_form_change() {
    let one = stand_of("r", &[("teile.teile", vec![("a", entry(&[("dmg", Leaf::Int(60))]))])]);
    let range = stand_of("r", &[("teile.teile", vec![("a", entry(&[("dmg[0]", Leaf::Int(1)), ("dmg[1]", Leaf::Int(60))]))])]);
    let found = changes_of(&compare(&one, &range));
    assert_eq!(found, [("dmg".to_string(), Some(Leaf::Int(60)), Some(text("[1, 60]")), Kind::Shape)]);
    let back = changes_of(&compare(&range, &one));
    assert_eq!(back, [("dmg".to_string(), Some(text("[1, 60]")), Some(Leaf::Int(60)), Kind::Shape)]);
    let cmp = compare(&one, &range);
    assert_eq!(cmp.classes().looks_different, 1, "a form is a thing that looks different");
}

#[test]
fn a_field_that_went_is_reported_as_gone() {
    let old = stand_of("r", &[("teile.teile", vec![("a", entry(&[("x", Leaf::Int(1)), ("y", Leaf::Int(2))]))])]);
    let new = stand_of("r", &[("teile.teile", vec![("a", entry(&[("x", Leaf::Int(1))]))])]);
    assert_eq!(changes_of(&compare(&old, &new)), [("y".to_string(), Some(Leaf::Int(2)), None, Kind::Shape)]);
    assert_eq!(changes_of(&compare(&new, &old)), [("y".to_string(), None, Some(Leaf::Int(2)), Kind::Shape)]);
}

#[test]
fn a_rename_hitting_many_entries_is_one_row_with_a_count() {
    let make = |rank: &str, extra: bool| {
        let entries: Vec<(String, Entry)> = (0..1000)
            .map(|i| {
                let mut values = vec![("rank", text(rank))];
                if extra && i == 5 {
                    values.push(("note", text("only here")));
                }
                (format!("e{i:04}"), entry(&values))
            })
            .collect();
        let mut stand = Stand::new("LIVE", "1", "2026-01-01", "r");
        let area = Area { entries: entries.into_iter().collect() };
        stand.areas.insert("auftragsruf.auftraege".into(), area);
        stand
    };
    let cmp = compare(&make("Cadet", false), &make("Recruit", true));
    let shared: Vec<&Row> = cmp.rows.iter().filter(|r| r.count == 1000).collect();
    assert_eq!(shared.len(), 1, "one row for the thousand");
    assert_eq!(shared[0].examples, ["e0000", "e0001", "e0002"], "three examples");
    assert_eq!(shared[0].changes.len(), 1);
    assert_eq!(cmp.rows.len(), 2, "and one for what only entry 5 has");
    assert_eq!(cmp.counts[0].changed, 1000);
    assert_eq!(cmp.counts[0].unchanged, 0);
    assert_eq!(cmp.rows[0].count, 1000, "the larger row first");
}

#[test]
fn a_placeholder_in_a_name_is_flagged() {
    let old = stand_of("r", &[("katalog.bauplaene", vec![("a", entry(&[("name", text("Alpha"))]))])]);
    let new = stand_of("r", &[("katalog.bauplaene", vec![("a", entry(&[("name", text("~mission(Title)"))]))])]);
    let cmp = compare(&old, &new);
    assert_eq!(changes_of(&cmp)[0].3, Kind::Shape, "a text that became a placeholder is a change of the form");
    assert_eq!(cmp.rows[0].class(), Class::LooksDifferent);
    let warning = cmp.warnings.iter().find(|w| w.name == PROBE_PLACEHOLDERS).expect("the counter grew");
    assert_eq!((warning.old, warning.new), (0, 1));
    let plain = stand_of("r", &[("katalog.bauplaene", vec![("a", entry(&[("name", text("Beta"))]))])]);
    assert_eq!(changes_of(&compare(&old, &plain))[0].3, Kind::Text, "an ordinary new name is only text");
}

#[test]
fn only_a_grown_gap_counter_warns() {
    let mut old = stand_of("r", &[]);
    let mut new = stand_of("r", &[]);
    old.probe.insert("failed".into(), 3);
    new.probe.insert("failed".into(), 3);
    assert!(compare(&old, &new).warnings.is_empty(), "the same counter says nothing");
    new.probe.insert("failed".into(), 2);
    assert!(compare(&old, &new).warnings.is_empty(), "a counter that fell says nothing");
    new.probe.insert("failed".into(), 5);
    let cmp = compare(&old, &new);
    assert_eq!(cmp.warnings, [Warning { name: "failed".into(), old: 3, new: 5, class: Class::Breaks }]);
    assert_eq!(cmp.classes().breaks, 1);
    new.probe.insert("failed".into(), 3);
    old.probe.remove(PROBE_PLACEHOLDERS);
    new.probe.insert(PROBE_PLACEHOLDERS.into(), 9);
    assert_eq!(compare(&old, &new).warnings.len(), 0, "a counter the older stand lacks is not compared");
}

#[test]
fn a_scale_rebuilt_is_its_own_row() {
    let scale = |levels: i64| {
        let entries: Vec<(String, Entry)> = (0..levels).map(|i| (format!("r{i:02}"), entry(&[("", Leaf::Int(i * 100))]))).collect();
        let mut stand = Stand::new("LIVE", "1", "2026-01-01", "r");
        let area = Area { entries: entries.into_iter().collect() };
        stand.areas.insert("rufstufen.stufen".into(), area);
        stand
    };
    let cmp = compare(&scale(51), &scale(5));
    assert_eq!(
        cmp.scales,
        [ScaleRow {
            area: "rufstufen.stufen".into(),
            old: Some(Scale { levels: 51, ceiling: Some(5000) }),
            new: Some(Scale { levels: 5, ceiling: Some(400) }),
        }]
    );
    assert!(cmp.scales[0].changed());
    assert!(!compare(&scale(5), &scale(5)).scales[0].changed());
    assert!(compare(&scale(5), &scale(5)).is_empty(), "a scale that stayed says nothing");
    assert!(cmp.classes().looks_different >= 1);
}

#[test]
fn a_field_of_a_struct_we_read_that_goes_away_breaks_and_a_new_one_does_not() {
    let forms = |structs: Vec<(&str, Entry)>| stand_of("r", &[(stands::AREA_FORMS, structs)]);
    let hp = |a: &str, b: &str| entry(&[("health", text(a)), ("armor", text(b))]);
    let before = forms(vec![("SHealthComponentParams", hp("Float", "Float")), ("GCSParams", entry(&[("g", text("Int"))]))]);
    // A field gone, one new: the gone one breaks, the new one only shows as a change.
    let lost_one = forms(vec![
        ("SHealthComponentParams", entry(&[("health", text("Float")), ("extra", text("Int"))])),
        ("GCSParams", entry(&[("g", text("Int"))])),
    ]);
    let cmp = compare(&before, &lost_one);
    let broken: Vec<_> = cmp.warnings.iter().filter(|w| w.name == FIELDS_GONE).collect();
    assert_eq!(broken.len(), 1);
    assert_eq!((broken[0].new, broken[0].class), (1, Class::Breaks));
    assert_eq!(cmp.classes().breaks, 1);
    // Only a new field: nothing breaks.
    let grew = forms(vec![
        ("SHealthComponentParams", entry(&[("health", text("Float")), ("armor", text("Float")), ("extra", text("Int"))])),
        ("GCSParams", entry(&[("g", text("Int"))])),
    ]);
    assert!(compare(&before, &grew).warnings.iter().all(|w| w.name != FIELDS_GONE));
    assert!(!compare(&before, &grew).is_empty(), "the new field is still told");
    // A whole struct that vanishes counts its fields.
    let vanished = forms(vec![("SHealthComponentParams", hp("Float", "Float"))]);
    let cmp = compare(&before, &vanished);
    assert_eq!(cmp.warnings.iter().find(|w| w.name == FIELDS_GONE).map(|w| w.new), Some(1));
    // The same shapes say nothing.
    assert!(compare(&before, &before).is_empty());
}

#[test]
fn an_older_reader_without_shapes_does_not_make_the_fields_look_gone() {
    let parts = || ("teile.teile", vec![("a", entry(&[("x", Leaf::Int(1))]))]);
    // The older reader had no shapes: the newer one brings them, nothing is gone.
    let old = stand_of("archiv-2", &[parts()]);
    let new = stand_of("archiv-3", &[parts(), (stands::AREA_FORMS, vec![("Gcs", entry(&[("g", text("Int"))]))])]);
    let cmp = compare(&old, &new);
    assert!(cmp.reader_changed);
    assert!(cmp.warnings.iter().all(|w| w.name != FIELDS_GONE));
    // The older reader had shapes and the newer one reads none: the readers differ, that is not a field gone.
    let (old, new) = (new, stand_of("archiv-4", &[parts()]));
    let cmp = compare(&old, &new);
    assert!(cmp.reader_changed);
    assert!(cmp.warnings.iter().all(|w| w.name != FIELDS_GONE && !w.name.starts_with("bereich:")), "{:?}", cmp.warnings);
}

#[test]
fn a_struct_without_fields_that_vanishes_counts_once() {
    let forms = |structs: Vec<(&str, Entry)>| stand_of("r", &[(stands::AREA_FORMS, structs)]);
    let before = forms(vec![("Empty", entry(&[])), ("Kept", entry(&[("k", text("Int"))]))]);
    let after = forms(vec![("Kept", entry(&[("k", text("Int"))]))]);
    let cmp = compare(&before, &after);
    assert_eq!(cmp.warnings.iter().find(|w| w.name == FIELDS_GONE).map(|w| w.new), Some(1));
}

#[test]
fn values_are_compared_exactly() {
    let with = |x: Leaf| stand_of("r", &[("teile.teile", vec![("a", entry(&[("rate", x)]))])]);
    let sum = Leaf::number(0.1 + 0.2);
    assert!(compare(&with(sum.clone()), &with(Leaf::number(0.1 + 0.2))).is_empty(), "the same bits are the same");
    assert_eq!(changes_of(&compare(&with(sum), &with(Leaf::number(0.3)))).len(), 1, "0.1 + 0.2 is not 0.3");
    assert!(compare(&with(Leaf::Int(60)), &with(Leaf::number(60.0))).is_empty(), "60 and 60.0 are the same number");
    assert_eq!(changes_of(&compare(&with(Leaf::Int(60)), &with(Leaf::number(60.5)))).len(), 1);
    assert_eq!(changes_of(&compare(&with(Leaf::Int(1)), &with(Leaf::Bool(true))))[0].3, Kind::Value);
}

#[test]
fn a_reader_of_another_version_compares_common_fields_only() {
    let old = stand_of(
        "reader-1",
        &[
            ("teile.teile", vec![("a", entry(&[("hp", Leaf::Int(1)), ("old_field", Leaf::Int(5))]))]),
            ("katalog.bauplaene", vec![("x", entry(&[("n", Leaf::Int(1))]))]),
        ],
    );
    let new = stand_of(
        "reader-2",
        &[
            ("teile.teile", vec![("a", entry(&[("hp", Leaf::Int(2)), ("new_field", Leaf::Int(7))]))]),
            ("schiffe.schiffe", vec![("s", entry(&[("n", Leaf::Int(1))]))]),
        ],
    );
    let cmp = compare(&old, &new);
    assert!(cmp.reader_changed);
    let plain: Vec<_> = cmp.rows.iter().filter(|r| !r.reader_note).collect();
    assert_eq!(plain.len(), 1);
    assert_eq!(plain[0].changes[0].path, "hp", "the common field is compared");
    let notes: Vec<&str> = cmp.rows.iter().filter(|r| r.reader_note).map(|r| r.changes[0].path.as_str()).collect();
    assert_eq!(notes, ["new_field", "old_field"], "the others are one row each, not gone and new");
    assert!(cmp.rows.iter().filter(|r| r.reader_note).all(|r| r.kind() == Kind::Shape && r.class() == Class::LooksDifferent));
    assert!(cmp.rows.iter().all(|r| r.fate == Fate::Changed), "an area of one reader is not a list of new or gone entries");
    // The same stands from the same reader: the fields are what the game did.
    let same = compare(&old, &Stand { reader: "reader-1".into(), ..new });
    assert!(!same.reader_changed);
    assert!(same.rows.iter().all(|r| !r.reader_note));
    assert!(same.rows.iter().any(|r| r.fate == Fate::Gone) && same.rows.iter().any(|r| r.fate == Fate::New));
}

#[test]
fn the_eigen_file_is_read_by_the_old_page_code() {
    let (paths, dir) = paths_in("eigen-file");
    let old = stand_of("r", &[("teile.teile", vec![("a", entry(&[("hp", Leaf::Int(10))])), ("b", entry(&[("hp", Leaf::Int(1))]))])]);
    let new = stand_of("r", &[("teile.teile", vec![("a", entry(&[("hp", Leaf::number(12.5))])), ("c", entry(&[("hp", Leaf::Int(5))]))])]);
    let cmp = compare(&old, &new);
    let file = write_patch(&paths, &cmp).expect("written").expect("says something");
    assert!(file.ends_with("eigen-4.10.0-LIVE.12345678.json"), "{file}");
    let version = "4.10.0-LIVE.12345678";
    assert_eq!(crate::patch_changes::stored(&paths), [version]);
    let items = crate::patch_changes::changes(&paths, version, None);
    let seen: Vec<(&str, crate::patch_changes::Fate, &str)> = items.iter().map(|c| (c.kind.as_str(), c.fate, c.id.as_str())).collect();
    assert_eq!(
        seen,
        [
            ("teile.teile", crate::patch_changes::Fate::New, "c"),
            ("teile.teile", crate::patch_changes::Fate::Gone, "b"),
            ("teile.teile", crate::patch_changes::Fate::Changed, "a"),
        ],
        "added, removed, modified, as erkul's files list them"
    );
    let changed = &items[2];
    assert_eq!((changed.fields[0].path.as_str(), changed.fields[0].old.clone(), changed.fields[0].new.clone()),
        ("hp", Some(Value::int(10)), Some(Value::Float(12.5))));
    let data = crate::patch_changes::load(&paths, version).unwrap();
    assert_eq!(crate::patch_changes::Summary::of(data.iter().find(|(k, _)| k == "summary").map(|(_, v)| v)),
        crate::patch_changes::Summary { added: 1, removed: 1, modified: 1 });
    assert_eq!(data.iter().find(|(k, _)| k == "quelle").and_then(|(_, v)| v.as_str()), Some("eigen"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_comparison_that_says_nothing_writes_no_file() {
    let (paths, dir) = paths_in("nothing");
    let stand = stand_of("r", &[("teile.teile", vec![("a", entry(&[("hp", Leaf::Int(10))]))])]);
    assert_eq!(write_patch(&paths, &compare(&stand, &stand.clone())), Ok(None));
    assert!(crate::patch_changes::stored(&paths).is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[cfg(not(feature = "test-build"))]
#[test]
fn the_user_build_writes_no_comparison_of_a_channel_it_is_not_built_for() {
    let (paths, dir) = paths_in("user-channel");
    let old = stand_of("r", &[("teile.teile", vec![("a", entry(&[("hp", Leaf::Int(10))]))])]);
    let new = stand_of("r", &[("teile.teile", vec![("a", entry(&[("hp", Leaf::Int(11))]))])]);
    let mut cmp = compare(&old, &new);
    cmp.to.channel = "EPTU".into();
    assert_eq!(write_patch(&paths, &cmp), Err(WriteError::ChannelNotBuiltIn));
    assert!(crate::patch_changes::stored(&paths).is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_comparison_of_ptu_is_not_marked_but_one_of_a_channel_under_an_agreement_is() {
    let old = stand_of("r", &[("teile.teile", vec![("a", entry(&[("hp", Leaf::Int(10))]))])]);
    let mut new = stand_of("r", &[("teile.teile", vec![("a", entry(&[("hp", Leaf::Int(11))]))])]);
    new.channel = "PTU".into();
    new.confidential = stands::is_confidential("PTU");
    assert!(!compare(&old, &new).to.confidential, "PTU is public");
    for channel in ["EPTU", "TECH-PREVIEW"] {
        new.channel = channel.into();
        new.confidential = stands::is_confidential(channel);
        // In the player build these channels are not known: nothing is marked there either.
        assert_eq!(compare(&old, &new).to.confidential, crate::edition::IS_TEST_BUILD, "{channel}");
    }
}

#[cfg(feature = "test-build")]
#[test]
fn a_comparison_marked_confidential_is_kept_marked() {
    let (paths, dir) = paths_in("confidential");
    let old = stand_of("r", &[("teile.teile", vec![("a", entry(&[("hp", Leaf::Int(10))]))])]);
    let mut new = stand_of("r", &[("teile.teile", vec![("a", entry(&[("hp", Leaf::Int(11))]))])]);
    new.channel = "EPTU".into();
    new.confidential = true;
    let cmp = compare(&old, &new);
    assert!(cmp.to.confidential);
    write_patch(&paths, &cmp).unwrap().unwrap();
    let list = crate::patch_changes::overview(&paths, &[]);
    assert_eq!(list.len(), 1);
    assert!(list[0].own && list[0].confidential && list[0].kept && !list[0].at_source);
    let live = compare(&old, &stand_of("r", &[("teile.teile", vec![("a", entry(&[("hp", Leaf::Int(12))]))])]));
    assert!(!live.to.confidential);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_text_area_is_counted_not_listed() {
    let hash = |n: i64| entry(&[("h", Leaf::Int(n))]);
    let old = stand_of("r", &[("texte.en", vec![("a", hash(1)), ("b", hash(2)), ("c", hash(3))])]);
    let new = stand_of("r", &[("texte.en", vec![("a", hash(1)), ("b", hash(9)), ("d", hash(4))])]);
    let cmp = compare(&old, &new);
    assert!(cmp.rows.is_empty(), "no row per text: {:?}", cmp.rows);
    let count = &cmp.counts[0];
    assert_eq!((count.added, count.removed, count.changed, count.unchanged), (1, 1, 1, 1));
    assert!(!cmp.is_empty(), "texts that changed are something to keep");
    assert_eq!(cmp.summary(), (0, 0, 0), "the list of patches counts what the lists name, not the texts");
}
