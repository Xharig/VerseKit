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

//! A run of the name gap filling against a real archive, for the developer (never part of a normal test run).

use std::path::Path;

use p4kx_dataforge::DataForge;

use versekit_core::injection::{self, Inputs, Outcome, names};
use versekit_core::jsonio::{self, Value};
use versekit_core::paths::{Host, Paths};
use versekit_core::pypath::Flavor;

use crate::{Language, name_needs};

/// Reads the archive `VERSEKIT_P4K`, lists the name keys its English original lacks, and writes them into copies of
/// the text files named in `VERSEKIT_INIS` (`language=path;language=path`, the copies are made in a temporary
/// folder); prints what was formed and how, and the lines of the copy that are new.
#[test]
#[ignore = "needs a real Data.p4k and text files, writes only into a temporary folder"]
fn the_real_archive_gets_its_missing_names_formed() {
    let Ok(archive) = std::env::var("VERSEKIT_P4K") else { return };
    let inis = std::env::var("VERSEKIT_INIS").expect("VERSEKIT_INIS");
    let game = crate::GameData::open_file(Path::new(&archive)).expect("archive");
    let texts = game.localization(Language::English).expect("texts");
    let bytes = game.dataforge_bytes().expect("dataforge");
    let forge = DataForge::open(&bytes).expect("forge");
    let listed = name_needs::read(&forge, &texts);
    println!("Namensschluessel, die das englische Original nicht hat: {}", listed.len());
    let catalog = Value::Object(vec![(names::FILE_KEY.to_string(), Value::Object(listed))]);
    let needs = names::needs_from_catalog(&catalog);
    for need in &needs {
        println!("  fehlt: {} ({}){}", need.key, need.kind, need.short_of.as_deref().map(|o| format!(" kurz von {o}")).unwrap_or_default());
    }
    for (number, entry) in inis.split(';').enumerate() {
        let (language, ini) = entry.split_once('=').expect("language=path");
        let root = std::env::temp_dir().join(format!("versekit-namen-{}-{number}", versekit_core::testdir::run_tag()));
        let root_text = root.to_string_lossy().into_owned();
        let data = format!("{root_text}/data");
        let target = format!("{root_text}/StarCitizen/LIVE/data/Localization/{language}/global.ini");
        std::fs::create_dir_all(&data).unwrap();
        std::fs::create_dir_all(Path::new(&target).parent().unwrap()).unwrap();
        std::fs::write(format!("{root_text}/StarCitizen/LIVE/Game.log"), "").unwrap();
        std::fs::copy(ini, &target).expect("copy of the text file");
        let before = std::fs::read_to_string(&target).unwrap();
        let host = Host::new(
            Flavor::Posix,
            &[("HOME", root_text.as_str()), ("VERSEKIT_HOME", &data), ("SC_INSTALL_DIR", &format!("{root_text}/StarCitizen/LIVE"))],
            &root_text,
        );
        let paths = Paths::new(host);
        let missions = jsonio::loads(
            r#"{"Real_title": {"bp": ["A"], "sicher": true, "chance": 1, "titel_key": "Real_title", "text_key": "Real_desc"}}"#,
        )
        .unwrap()
        .as_object()
        .unwrap()
        .clone();
        let inputs = Inputs { missions, name_needs: needs.clone(), ..Inputs::default() };
        let outcome = injection::apply_texts(&paths, &target, language, &inputs, false);
        assert!(matches!(outcome, Outcome::Done(_)), "{outcome:?}");
        let after = std::fs::read_to_string(&target).unwrap();
        println!("\n== {language}: {ini}");
        let old: std::collections::HashSet<&str> = before.lines().collect();
        let added = injection::added_names(&paths, &target);
        println!("ergaenzte Namen: {}", added.len());
        for name in &added {
            println!("  {} = {}   [{}, aus {}]", name.key, name.text, name.how.code(), name.from);
        }
        let key_of = |line: &str| line.split(['=', ',']).next().unwrap_or("").to_string();
        let have: std::collections::HashSet<String> = before.lines().map(key_of).collect();
        let new_lines: Vec<&str> = after.lines().filter(|l| !old.contains(l) && !have.contains(&key_of(l))).collect();
        println!("neue Zeilen in der Kopie: {}", new_lines.len());
        for line in new_lines {
            println!("  {line}");
        }
        injection::remove_texts(&paths, &target, language);
        let removed = std::fs::read_to_string(&target).unwrap();
        for name in &added {
            assert!(!removed.contains(&format!("\n{}=", name.key)), "{} is gone again", name.key);
        }
        assert!(injection::added_names(&paths, &target).is_empty());
    }
}
