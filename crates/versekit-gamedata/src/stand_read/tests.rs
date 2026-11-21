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

use std::fs;

use super::*;
use versekit_core::stands::Leaf;

fn host_in(dir: &Path) -> Host {
    let root = dir.to_string_lossy().into_owned();
    Host::new(versekit_core::pypath::Flavor::native(), &[(versekit_core::envvars::ENV_HOME, root.as_str())], &root)
}

/// An archive path in a folder of the channel `channel`; the file need not exist.
fn archive_of(dir: &Path, channel: &str) -> PathBuf {
    dir.join(channel).join("Data.p4k")
}

/// The six cache files with invented content of one build: the catalog, the values, the ships, the parts, the
/// reputation and the ranks.
fn write_caches(dir: &Path, build: &str, health: i64, levels: i64) {
    let steps: Vec<String> = (0..levels).map(|i| format!("\"r{i}\": {}", i * 100)).collect();
    let files = [
        (
            "katalog-spieldaten.json",
            format!(
                r#"{{"quelle": {{"archiv_stand": {{"groesse": "7"}}}}, "version": "{build}",
                "bauplaene": {{"bp1": {{"n": "Alpha", "s": 1}}}}, "missionen": {{"m1": {{"bp": ["Alpha"], "rep_max": 5}}}}}}"#
            ),
        ),
        (
            "werte-spieldaten.json",
            format!(
                r#"{{"stamp": {{"a": 1}}, "values": {{"build": "{build}", "items": {{"i1": {{"hp": {health}}}}},
                "recipes": [], "failed": []}}}}"#
            ),
        ),
        ("spiel-schiffe.json", r#"{"format": 4, "schiffe": {"s1": {"id": "S1", "name": "Ship"}}}"#.to_string()),
        ("spiel-teile.json", r#"{"format": 1, "teile": {"p1": {"size": 2}}}"#.to_string()),
        (
            "auftragsruf.json",
            r#"{"format": 1, "version": "x", "auftraege": {"c1": {"rep": 5}}, "regionen": {}, "angaben": {}}"#.to_string(),
        ),
        ("rufstufen.json", format!(r#"{{"format": 2, "version": "x", "stufen": {{{}}}}}"#, steps.join(", "))),
    ];
    for (name, content) in files {
        fs::write(dir.join(name), content).unwrap();
    }
}

fn cache_file(dir: &Path) -> String {
    dir.join("katalog-spieldaten.json").to_string_lossy().into_owned()
}

/// The stand a reading would give of the caches now, made as [`read_stand`] makes it from the areas.
fn stand_of_caches(dir: &Path, channel: &str, build: &str) -> Stand {
    let files: Vec<(&CacheFile, String)> =
        CACHES.iter().map(|c| (c, dir.join(c.file).to_string_lossy().into_owned())).collect();
    let mut stand = Stand::from_cache_files(&files, channel, build, "2026-11-15").expect("caches");
    stand.reader = READER.into();
    stand
}

#[test]
fn the_old_stand_is_taken_from_the_caches_before_they_are_overwritten() {
    let dir = versekit_core::testdir::fresh("stand-vor-dem-ueberschreiben");
    let (host, archive) = (host_in(&dir), archive_of(&dir, "LIVE"));
    let paths = Paths::new(host.clone());
    write_caches(&dir, "4.9.0-live.100", 10, 3);

    assert!(keep_base(&host, &archive, &cache_file(&dir)), "the base is kept");
    assert!(!keep_base(&host, &archive, &cache_file(&dir)), "not twice for the same build");
    let listed = stands::list(&paths);
    assert_eq!(listed.len(), 1);
    assert_eq!((listed[0].channel.as_str(), listed[0].build.as_str()), ("LIVE", "4.9.0-live.100"));

    // The rebuild writes the caches anew: the base keeps what they held.
    write_caches(&dir, "4.10.0-live.200", 12, 3);
    let base = stands::load(&paths, "LIVE", "4.9.0-live.100").expect("base");
    assert_eq!(base.reader, stands::READER_CACHES);
    assert_eq!(base.areas["werte.items"].entries["i1"]["hp"], Leaf::Int(10), "what the caches held, not what they hold now");

    let comparison = finish(&paths, stand_of_caches(&dir, "LIVE", "4.10.0-live.200")).expect("a comparison with the base");
    assert!(comparison.reader_changed, "the base is of the caches, the new one of a reading");
    assert_eq!(comparison.from.build, "4.9.0-live.100");
    assert_eq!(comparison.to.build, "4.10.0-live.200");
    let hp: Vec<_> = comparison.rows.iter().filter(|r| r.area == "werte.items").collect();
    assert_eq!(hp.len(), 1, "{:?}", comparison.rows);
    assert_eq!(hp[0].changes[0].old, Some(Leaf::Int(10)));
    assert_eq!(hp[0].changes[0].new, Some(Leaf::Int(12)));
    assert_eq!(versekit_core::patch_changes::stored(&paths), ["4.10.0-live.200"], "the page reads the comparison");
    assert_eq!(stands::list(&paths).len(), 2, "the new stand is kept next to the base");
}

#[test]
fn a_taking_of_the_base_after_the_overwrite_would_show_nothing() {
    // The reverse of the test above, as the reason for the order: a base taken from the new caches is the new stand.
    let dir = versekit_core::testdir::fresh("stand-nach-dem-ueberschreiben");
    let paths = Paths::new(host_in(&dir));
    write_caches(&dir, "4.10.0-live.200", 12, 3);
    let late = stand_of_caches(&dir, "LIVE", "4.9.0-live.100");
    let new = stand_of_caches(&dir, "LIVE", "4.10.0-live.200");
    assert!(stand_diff::compare(&late, &new).is_empty(), "nothing to tell: the old state is lost for good");
    drop(paths);
}

#[test]
fn the_first_stand_only_lays_the_base() {
    let dir = versekit_core::testdir::fresh("stand-erster");
    let paths = Paths::new(host_in(&dir));
    write_caches(&dir, "4.10.0-live.200", 12, 3);
    assert!(finish(&paths, stand_of_caches(&dir, "LIVE", "4.10.0-live.200")).is_none(), "nothing to compare with");
    assert_eq!(stands::list(&paths).len(), 1);
    assert!(versekit_core::patch_changes::stored(&paths).is_empty(), "no comparison file");
    // The same build read again is no new patch.
    assert!(finish(&paths, stand_of_caches(&dir, "LIVE", "4.10.0-live.200")).is_none());
    assert_eq!(stands::list(&paths).len(), 1);
}

#[test]
fn a_comparison_runs_from_the_stand_before_not_from_any_other_channel() {
    let dir = versekit_core::testdir::fresh("stand-kanaele");
    let paths = Paths::new(host_in(&dir));
    write_caches(&dir, "4.9.0-live.100", 10, 3);
    assert!(finish(&paths, stand_of_caches(&dir, "PTU", "5.0.0-ptu.50")).is_none());
    write_caches(&dir, "4.10.0-live.200", 12, 3);
    assert!(finish(&paths, stand_of_caches(&dir, "LIVE", "4.10.0-live.200")).is_none(), "another channel is no base");
    write_caches(&dir, "4.10.1-live.210", 14, 3);
    let comparison = finish(&paths, stand_of_caches(&dir, "LIVE", "4.10.1-live.210")).expect("the live stand before");
    assert_eq!(comparison.from.build, "4.10.0-live.200");
}

#[cfg(not(feature = "test-build"))]
#[test]
fn the_user_build_makes_no_stand_of_a_channel_it_does_not_know() {
    let dir = versekit_core::testdir::fresh("stand-fremder-kanal");
    let host = host_in(&dir);
    write_caches(&dir, "5.0.0-eptu.1", 10, 3);
    for channel in ["EPTU", "TECH-PREVIEW"] {
        assert!(!keep_base(&host, &archive_of(&dir, channel), &cache_file(&dir)), "{channel}");
    }
    let mut stand = stand_of_caches(&dir, "PTU", "5.0.0-eptu.1");
    stand.channel = "EPTU".into();
    assert!(finish(&Paths::new(host.clone()), stand).is_none());
    assert!(stands::list(&Paths::new(host)).is_empty(), "nothing kept");
}

#[test]
fn a_number_stays_what_the_reader_gave_it() {
    let json = serde_json::json!({"whole": 1, "fraction": 2.5, "even": 10.0, "big": u64::MAX, "list": [true, null, "x"]});
    let value = from_serde(&json);
    let read = jsonio::loads(&serde_json::to_string(&json).unwrap()).unwrap();
    assert_eq!(value.get("whole"), Some(&Value::int(1)));
    assert_eq!(value.get("fraction"), Some(&Value::Float(2.5)));
    assert_eq!(value.get("even"), Some(&Value::Float(10.0)), "a number with a fraction stays one");
    assert_eq!(value.get("big"), Some(&Value::Int(u64::MAX.to_string())));
    // What serde gives and what the reading of the written file gives are the same value: the stand of a reading and
    // the stand of the kept cache are made of the same numbers.
    for key in ["whole", "fraction", "even", "big", "list"] {
        assert_eq!(value.get(key), read.get(key), "{key}");
    }
}

#[test]
fn the_channel_is_the_folder_of_the_archive() {
    let at = |folder: &str| channel_of_archive(&archive_of(Path::new("x"), folder));
    assert_eq!(at("LIVE").as_deref(), Some("LIVE"));
    assert_eq!(at("PTU").as_deref(), Some("PTU"));
    assert_eq!(at("Spieldaten"), None, "a folder that is no channel");
}

#[test]
fn the_checksum_of_a_text_is_the_same_in_every_version() {
    // FNV-1a, 64 bit: the known values of the empty text and of "a", as the numbers with the same bits.
    assert_eq!(checksum(""), i64::from_ne_bytes(0xcbf2_9ce4_8422_2325_u64.to_ne_bytes()));
    assert_eq!(checksum("a"), i64::from_ne_bytes(0xaf63_dc4c_8601_ec8c_u64.to_ne_bytes()));
    assert_ne!(checksum("Shield"), checksum("shield"));
}

#[test]
fn the_texts_of_an_ini_become_checksums_per_key_and_the_missing_translations_are_counted() {
    let ini = "\u{feff}item_name_a=Alpha\r\n; a comment\r\nitem_name_b=TRANSLATION NOT FOUND\r\n\r\nitem_desc_a,P=Line\\nTwo\r\n";
    let (entries, not_found) = texts_of(ini.as_bytes());
    assert_eq!(entries.keys().collect::<Vec<_>>(), ["item_desc_a,P", "item_name_a", "item_name_b"]);
    assert_eq!(not_found, 1);
    assert_eq!(entries["item_name_a"]["h"], Leaf::Int(checksum("Alpha")));
    let (again, _) = texts_of(ini.as_bytes());
    assert_eq!(entries, again, "the same text, the same checksums");
}
