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

fn paths_in(name: &str) -> (Paths, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!("versekit-stands-{}-{name}", crate::testdir::run_tag()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let home = dir.to_string_lossy().into_owned();
    (Paths::new(Host::new(Flavor::native(), &[(crate::envvars::ENV_HOME, home.as_str())], &home)), dir)
}

fn entry(values: &[(&str, Leaf)]) -> Entry {
    values.iter().map(|(path, leaf)| ((*path).to_string(), leaf.clone())).collect()
}

fn text(s: &str) -> Leaf {
    Leaf::Text(s.to_string())
}

/// An invented stand with one area of two entries.
fn stand(channel: &str, build: &str) -> Stand {
    let mut stand = Stand::new(channel, build, "2026-01-01", "reader-1");
    let mut area = Area::default();
    area.entries.insert("a".into(), entry(&[("name", text("Alpha")), ("size", Leaf::Int(60))]));
    area.entries.insert("b".into(), entry(&[("name", text("Beta")), ("rate", Leaf::number(0.1 + 0.2))]));
    stand.areas.insert("katalog.bauplaene".into(), area);
    stand.probe = probe_of(&stand.areas);
    stand
}

/// Writes a stand file without asking the channel, as a file of another build or a copy would be there.
fn put_without_guard(paths: &Paths, stand: &Stand) {
    let packed = miniz_oxide::deflate::compress_to_vec_zlib(crate::patch_changes::compact(&stand.to_value()).as_bytes(), 6);
    std::fs::write(path_of(paths, &stand.channel, &stand.build), packed).unwrap();
}

const CATALOG: &str = r#"{"quelle": {"format": 10, "archiv": "x.p4k", "archiv_stand": {"size": 7, "time": 8}},
    "bauplaene": [{"key": "b1", "name": "Alpha", "s": 1, "seit": "4.9"}, {"key": "b1", "name": "Alpha again"},
                  {"name": "~mission(Title)"}],
    "missionen": {"m1": {"bp": ["Alpha"], "sicher": 1, "chance": 0.5, "uec": 100, "cooldown": 5}},
    "beschreibungen": {"g1": {"name": "N", "text": ""}}}"#;
const VALUES: &str = r#"{"stamp": {"a": 1}, "values": {"build": "4.1", "items": {"i1": {"hp": 10.5}},
    "recipes": [{"tag": "r1", "tiers": [{"craftTimeSeconds": 5}]}], "failed": ["bad record"]}}"#;
const SHIPS: &str = r#"{"format": 4, "spielversion": "4.1", "hersteller": {"a": "b"},
    "schiffe": {"s1": {"id": "S", "plaetze": [1, 2]}}}"#;
const PARTS: &str = r#"{"format": 1, "teile": {"p1": {"size": 2}}}"#;
const REPUTATION: &str = r#"{"format": 1, "version": "4.1", "auftraege": {"c1": {"rep": 5}}, "regionen": {},
    "datenfehler": "x", "angaben": {"k": "v"}, "angaben_quelle": "spieldaten"}"#;
const RANKS: &str = r#"{"format": 2, "version": "4.1", "stufen": {"r1": 0, "r2": 100}}"#;

fn write_caches(paths: &Paths) {
    for (cache, content) in CACHES.iter().zip([CATALOG, VALUES, SHIPS, PARTS, REPUTATION, RANKS]) {
        std::fs::write(paths.app_file(cache.file), content).unwrap();
    }
}

#[test]
fn a_stand_survives_saving_and_loading_exactly() {
    let (paths, dir) = paths_in("roundtrip");
    let mut kept = stand("LIVE", "4.10.0-LIVE.1");
    let area = kept.areas.get_mut("katalog.bauplaene").unwrap();
    area.entries.insert(
        "c".into(),
        entry(&[
            ("whole", Leaf::number(60.0)),
            ("yes", Leaf::Bool(true)),
            ("none", Leaf::Null),
            ("empty", text("[]")),
            ("big", Leaf::Int(i64::MIN)),
            ("quote", text("a \"b\" \\ ü")),
        ]),
    );
    kept.archive_stamp = "{\"size\":1}".into();
    save(&paths, &kept).expect("saved");
    let loaded = load(&paths, "LIVE", "4.10.0-LIVE.1").expect("loaded");
    assert_eq!(loaded, kept, "every value comes back, a number with the bits it had");
    let bits = |s: &Stand| s.areas["katalog.bauplaene"].entries["b"]["rate"].clone();
    assert_eq!(bits(&loaded), Leaf::Num((0.1_f64 + 0.2).to_bits()), "0.1 + 0.2 is not 0.3");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_marks_follow_from_the_channel_not_from_the_file() {
    let live = stand("LIVE", "1");
    let ptu = stand("PTU", "1");
    assert!(!live.confidential && !live.under_agreement);
    assert!(!ptu.confidential && !ptu.under_agreement, "PTU is public");
    let mut value = live.to_value();
    jsonio::map_set(match &mut value { Value::Object(map) => map, _ => unreachable!() }, "vertraulich", Value::Bool(true));
    assert!(!Stand::from_value(&value).unwrap().confidential, "what the file says does not count");
    // A stand of PTU written when PTU was still marked keeps no mark: the channel decides.
    let mut value = ptu.to_value();
    jsonio::map_set(match &mut value { Value::Object(map) => map, _ => unreachable!() }, "vertraulich", Value::Bool(true));
    assert!(!Stand::from_value(&value).unwrap().confidential, "an old mark on PTU does not count");
    assert_eq!(ptu.to_value().get("vertraulich"), Some(&Value::Bool(false)), "the file carries no mark for PTU");
    // The channels under an agreement are confidential, the others never (in the test build, where they exist).
    if crate::edition::IS_TEST_BUILD {
        for channel in ["EPTU", "TECH-PREVIEW"] {
            let secret = stand(channel, "1");
            assert!(secret.confidential && secret.under_agreement, "{channel}");
            assert_eq!(secret.to_value().get("vertraulich"), Some(&Value::Bool(true)), "{channel}");
            let mut value = secret.to_value();
            jsonio::map_set(match &mut value { Value::Object(map) => map, _ => unreachable!() }, "vertraulich", Value::Bool(false));
            assert!(Stand::from_value(&value).unwrap().confidential, "{channel}: the channel decides");
        }
    }
    assert!(!is_confidential("PTU") && !is_confidential("HOTFIX") && !is_confidential("LIVE"));
}

#[test]
fn only_a_channel_under_an_agreement_is_confidential() {
    for channel in crate::paths::UNDER_AGREEMENT {
        assert!(is_confidential(channel), "{channel}");
    }
    for channel in crate::paths::CHANNELS {
        assert_eq!(is_confidential(channel), crate::paths::UNDER_AGREEMENT.contains(&channel), "{channel}");
    }
}

#[cfg(not(feature = "test-build"))]
#[test]
fn the_user_build_keeps_no_stand_of_a_channel_under_an_agreement() {
    let (paths, dir) = paths_in("user-channels");
    for channel in ["EPTU", "TECH-PREVIEW"] {
        assert!(!built_in(channel), "{channel}");
        assert_eq!(save(&paths, &stand(channel, "1")), Err(SaveError::ChannelNotBuiltIn), "{channel}");
        // A file of that channel that got here some other way is neither listed nor loaded.
        put_without_guard(&paths, &stand(channel, "1"));
        assert!(list(&paths).is_empty(), "{channel}");
        assert!(load(&paths, channel, "1").is_none(), "{channel}");
    }
    assert!(save(&paths, &stand("PTU", "1")).is_ok(), "PTU stays, public");
    assert_eq!(list(&paths).len(), 1);
    let _ = std::fs::remove_dir_all(&dir);
}

#[cfg(feature = "test-build")]
#[test]
fn the_test_build_keeps_the_stands_of_all_its_channels_marked() {
    let (paths, dir) = paths_in("test-channels");
    for channel in ["PTU", "EPTU", "TECH-PREVIEW"] {
        save(&paths, &stand(channel, "1")).unwrap_or_else(|e| panic!("{channel}: {e:?}"));
        let loaded = load(&paths, channel, "1").expect(channel);
        assert_eq!(loaded.confidential, channel != "PTU", "{channel}");
    }
    assert!(load(&paths, "EPTU", "1").unwrap().under_agreement);
    assert!(load(&paths, "TECH-PREVIEW", "1").unwrap().under_agreement);
    assert!(!load(&paths, "PTU", "1").unwrap().under_agreement);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn old_stands_are_pruned_per_channel() {
    let (paths, dir) = paths_in("prune");
    for n in 1..=6 {
        save(&paths, &stand("LIVE", &format!("4.10.{n}-LIVE.{}", 1000 + n))).unwrap();
    }
    for n in 1..=2 {
        save(&paths, &stand("PTU", &format!("5.0.{n}-PTU.{}", 2000 + n))).unwrap();
    }
    let builds = |channel: &str| -> Vec<String> {
        list(&paths).into_iter().filter(|i| i.channel == channel).map(|i| i.build).collect()
    };
    assert_eq!(
        builds("LIVE"),
        ["4.10.6-LIVE.1006", "4.10.5-LIVE.1005", "4.10.4-LIVE.1004", "4.10.3-LIVE.1003"],
        "the newest four of the channel stay, newest first"
    );
    assert_eq!(builds("PTU").len(), 2, "another channel is counted apart");
    assert!(load(&paths, "LIVE", "4.10.1-LIVE.1001").is_none(), "the oldest is gone");
    assert!(load(&paths, "LIVE", "4.10.6-LIVE.1006").is_some(), "the newest stays");

    let dropped = prune_with(&paths, 2, MAX_BYTES);
    assert_eq!(dropped.len(), 2, "{dropped:?}");
    assert_eq!(builds("LIVE"), ["4.10.6-LIVE.1006", "4.10.5-LIVE.1005"]);

    let one = list(&paths)[0].bytes;
    prune_with(&paths, 4, one + 1);
    let left = list(&paths);
    assert_eq!(left.len(), 2, "the limit of the folder leaves the newest stand of each channel: {left:?}");
    assert_eq!(left.iter().map(|i| i.channel.as_str()).collect::<Vec<_>>(), ["LIVE", "PTU"]);
    assert_eq!(left[0].build, "4.10.6-LIVE.1006");
    assert_eq!(left[1].build, "5.0.2-PTU.2002");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_caches_make_a_stand_without_their_header() {
    let (paths, dir) = paths_in("caches");
    assert!(Stand::from_caches(&paths, "LIVE", "4.1", "2026-01-01").is_none(), "no cache, no stand");
    write_caches(&paths);
    let stand = Stand::from_caches(&paths, "LIVE", "4.1", "2026-01-01").expect("stand");
    assert_eq!(stand.reader, READER_CACHES);
    let names: Vec<&str> = stand.areas.keys().map(String::as_str).collect();
    assert_eq!(
        names,
        [
            "auftragsruf.angaben",
            "auftragsruf.auftraege",
            "auftragsruf.regionen",
            "katalog.bauplaene",
            "katalog.beschreibungen",
            "katalog.missionen",
            "rufstufen.stufen",
            "schiffe.hersteller",
            "schiffe.schiffe",
            "teile.teile",
            "werte.failed",
            "werte.items",
            "werte.recipes",
        ],
        "format, source, stamp, version and build are no areas, the wrapper is looked through"
    );
    let list = &stand.areas["katalog.bauplaene"].entries;
    let keys: Vec<&str> = list.keys().map(String::as_str).collect();
    assert_eq!(keys, ["b1", "b1#2", "~mission(Title)"], "a repeated key gets a number, the name names an entry without a key");
    assert_eq!(list["b1"].get("seit"), None, "what the device works out itself is left out");
    assert_eq!(list["b1"]["s"], Leaf::Int(1));
    let mission: Vec<&str> = stand.areas["katalog.missionen"].entries["m1"].keys().map(String::as_str).collect();
    assert_eq!(mission, ["bp[0]", "cooldown"], "what only scmdb fills in is left out");
    assert_eq!(stand.areas["schiffe.schiffe"].entries["s1"]["plaetze[1]"], Leaf::Int(2));
    assert_eq!(stand.areas["rufstufen.stufen"].entries["r2"][""], Leaf::Int(100), "a plain value is the leaf without a path");
    assert!(stand.areas["auftragsruf.regionen"].entries.is_empty());
    assert!(!stand.archive_stamp.is_empty(), "the stamp of the archive is taken from the catalog");
    assert_eq!(stand.probe[PROBE_FAILED], 1);
    assert_eq!(stand.probe[PROBE_PLACEHOLDERS], 1);
    assert_eq!(stand.probe[PROBE_EMPTY_TEXTS], 1);
    let _ = std::fs::remove_dir_all(&dir);
}

/// The error report, the export and the files of the Basetool never read a stand or an own comparison: what a stand
/// holds is game content, a test channel's under confidence, and these are the ways a file leaves the computer.
#[test]
fn no_report_export_or_basetool_code_reads_the_stands() {
    let crates = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let mut stack: Vec<std::path::PathBuf> =
        std::fs::read_dir(crates).unwrap().flatten().map(|e| e.path().join("src")).collect();
    let (mut seen, mut found) = (0, Vec::new());
    while let Some(folder) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&folder) else { continue };
        for path in entries.flatten().map(|e| e.path()) {
            let unix = path.to_string_lossy().replace('\\', "/");
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            let leaves = ["report", "redact", "export", "basetool"];
            let name = unix.rsplit("/src/").next().unwrap_or(&unix);
            if !path.extension().is_some_and(|e| e == "rs") || !leaves.iter().any(|leaf| name.contains(leaf)) {
                continue;
            }
            seen += 1;
            let text = std::fs::read_to_string(&path).unwrap();
            if ["stands::", "stand_diff", "Staende", "eigen-"].iter().any(|word| text.contains(word)) {
                found.push(unix);
            }
        }
    }
    assert!(seen > 10, "the source was read ({seen} files)");
    assert!(found.is_empty(), "these read the stands: {found:#?}");
}

#[test]
fn a_stand_of_another_layout_or_a_broken_file_is_no_stand() {
    let (paths, dir) = paths_in("broken");
    let mut value = stand("LIVE", "1").to_value();
    jsonio::map_set(match &mut value { Value::Object(map) => map, _ => unreachable!() }, "format", Value::int(FORMAT + 1));
    assert!(Stand::from_value(&value).is_none());
    std::fs::write(path_of(&paths, "LIVE", "2"), b"not a stand").unwrap();
    assert!(load(&paths, "LIVE", "2").is_none());
    assert_eq!(list(&paths).len(), 1, "listed as a file, not loadable");
    let _ = std::fs::remove_dir_all(&dir);
}
