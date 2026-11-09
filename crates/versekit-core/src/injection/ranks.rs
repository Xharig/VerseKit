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

//! From which reputation a rank begins, for the rank names in the
//! reputation menu (`Gildenmitglied [ab 10.000]`): port of
//! `rank_thresholds.py`.
//!
//! The numbers come from the player's `Data.p4k` (the reputation scopes and
//! their standings, read by `versekit_gamedata::reputation_ranks`); only the
//! result is stored (`rufstufen.json`), bound to the game version.

use std::sync::LazyLock;

use regex::Regex;

use crate::errors;
use crate::jsonio::{self, Map, Value};
use crate::paths::Paths;

pub const CACHE_FILE: &str = "rufstufen.json";
/// From 2 read from the game data; a table of format 1 is read anew once.
pub const FORMAT: i64 = 2;
/// Standings from this threshold get a number.
pub const MIN_SHOWN: i64 = 1;
/// The setting „Ruf-Stufen an den Rängen“.
pub const SETTING: &str = "ruf_stufen";

/// Our addition at the end of a rank name (`rank_thresholds._SUFFIX`).
static SUFFIX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(&format!(r"{}\[(?:ab [\d.]+|[\d,]+\+)\]$", crate::text::PY_SPACE)).expect("pattern"));

/// The oldest format that is still read: its thresholds count until they are read anew.
const OLDEST_FORMAT: i64 = 1;

/// The stored thresholds: game version and key → threshold, in file order.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Stored {
    pub version: String,
    pub thresholds: Vec<(String, Value)>,
    /// The format of the file; below [`FORMAT`] the thresholds are read anew at the next chance.
    pub format: i64,
}

/// `rank_thresholds.load`.
pub fn load(paths: &Paths) -> Stored {
    let Ok(data) = jsonio::read_json(&paths.app_file(CACHE_FILE)) else { return Stored::default() };
    let format = match data.get("format") {
        Some(Value::Int(text)) => text.parse().unwrap_or(0),
        _ => 0,
    };
    match data.get("stufen") {
        Some(Value::Object(map)) if (OLDEST_FORMAT..=FORMAT).contains(&format) => Stored {
            version: data.get("version").and_then(Value::as_str).unwrap_or("").to_string(),
            thresholds: map.clone(),
            format,
        },
        _ => Stored::default(),
    }
}

/// Writes the thresholds of `version`; whether it worked.
pub fn save(paths: &Paths, version: &str, table: &[(String, i64)]) -> bool {
    let data: Map = vec![
        ("format".into(), Value::int(FORMAT)),
        ("version".into(), Value::str(version)),
        ("stufen".into(), Value::Object(table.iter().map(|(k, v)| (k.clone(), Value::int(*v))).collect())),
    ];
    let target = paths.app_file(CACHE_FILE);
    let text = jsonio::dumps(&Value::Object(data), None, false);
    match jsonio::write_text_atomic(&target, &text, jsonio::Newline::Keep) {
        Ok(()) => true,
        Err(error) => {
            errors::record("rank_thresholds.save", &error);
            false
        }
    }
}

/// `rank_thresholds.refresh`: reads the thresholds with `read` when none are stored, they belong to another game
/// version or stand in an older format. Returns the number of keys; when `read` gives nothing the old state stays.
pub fn refresh(paths: &Paths, game_version: &str, read: &dyn Fn() -> Option<Vec<(String, i64)>>) -> usize {
    let old = load(paths);
    let current = old.format == FORMAT && (game_version.is_empty() || old.version == game_version);
    if !old.thresholds.is_empty() && current {
        return old.thresholds.len();
    }
    let Some(table) = read().filter(|table| !table.is_empty()) else { return old.thresholds.len() };
    save(paths, game_version, &table);
    table.len()
}

fn grouped(value: i64, sep: &str) -> String {
    super::texts::group_digits(i128::from(value), sep)
}

/// `rank_thresholds.suffix`: ` [ab 10.000]` in German, ` [10,000+]` else.
pub fn suffix(value: i64, lang_code: &str) -> String {
    if lang_code == "de" { format!(" [ab {}]", grouped(value, ".")) } else { format!(" [{}+]", grouped(value, ",")) }
}

/// `rank_thresholds.strip_suffix`.
pub fn strip_suffix(text: &str) -> String {
    SUFFIX.replace(text, "").into_owned()
}

/// `rank_thresholds.with_suffix`.
pub fn with_suffix(text: &str, tail: &str) -> String {
    format!("{}{tail}", crate::text::py_rstrip(&strip_suffix(text)))
}

/// `rank_thresholds.build_table`: language key → addition; empty when off.
pub fn build_table(paths: &Paths, lang_code: &str, stored: &Stored) -> Vec<(String, String)> {
    if !paths.setting_bool(SETTING, true) {
        return Vec::new();
    }
    stored
        .thresholds
        .iter()
        .filter_map(|(key, value)| match value {
            Value::Int(text) => {
                text.parse::<i64>().ok().filter(|v| *v >= MIN_SHOWN).map(|v| (key.clone(), suffix(v, lang_code)))
            }
            _ => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The thresholds are read once per game version; nothing read keeps the old ones.
    #[test]
    fn the_thresholds_are_read_once_per_game_version() {
        use std::cell::Cell;

        use crate::paths::Host;
        use crate::pypath::Flavor;
        let dir = std::env::temp_dir().join(format!("versekit-raenge-{}", crate::testdir::run_tag()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let data = dir.to_string_lossy().into_owned();
        let paths = Paths::new(Host::new(Flavor::native(), &[("SC_BP_HOME", data.as_str())], "/"));
        let reads = Cell::new(0);
        let table = |value: i64| {
            reads.set(reads.get() + 1);
            Some(vec![("RepStanding_Barter_Rank1_Name".to_string(), value)])
        };
        assert_eq!(refresh(&paths, "4.3", &|| None), 0);
        assert_eq!(refresh(&paths, "4.3", &|| table(340)), 1);
        assert_eq!(refresh(&paths, "4.3", &|| table(1)), 1);
        assert_eq!(reads.get(), 1, "the same version is not read again");
        assert_eq!(load(&paths).thresholds, vec![("RepStanding_Barter_Rank1_Name".to_string(), Value::int(340))]);
        assert_eq!(load(&paths).format, FORMAT);
        assert_eq!(refresh(&paths, "4.4", &|| Some(Vec::new())), 1, "an empty reading keeps the old table");
        assert_eq!(load(&paths).version, "4.3");
        refresh(&paths, "4.4", &|| table(500));
        assert_eq!(load(&paths).version, "4.4");
        let _ = std::fs::remove_dir_all(dir);
    }

    /// A table of the older format (as 5.2.6 wrote it from scunpacked) counts until the game data can be read, and is
    /// read anew once even for the same game version.
    #[test]
    fn a_table_of_the_old_format_counts_until_it_is_read_anew() {
        use std::cell::Cell;

        use crate::paths::Host;
        use crate::pypath::Flavor;
        let dir = std::env::temp_dir().join(format!("versekit-raenge-alt-{}", crate::testdir::run_tag()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let data = dir.to_string_lossy().into_owned();
        let paths = Paths::new(Host::new(Flavor::native(), &[("SC_BP_HOME", data.as_str())], "/"));
        let old = r#"{"format": 1, "version": "4.10.2", "stufen": {"RepStanding_Barter_Rank1_Name": 340}}"#;
        std::fs::write(paths.app_file(CACHE_FILE), old).unwrap();
        assert_eq!(load(&paths).thresholds.len(), 1, "the old table is not empty");
        // The game runs: nothing can be read, the old table stays.
        assert_eq!(refresh(&paths, "4.10.2", &|| None), 1);
        assert_eq!(load(&paths).format, 1);
        let reads = Cell::new(0);
        refresh(&paths, "4.10.2", &|| {
            reads.set(reads.get() + 1);
            Some(vec![("RepStanding_Barter_Rank1_Name".to_string(), 340), ("RepScope_Contractor_Rank1".to_string(), 800)])
        });
        assert_eq!(reads.get(), 1, "the same game version is read anew for the new format");
        assert_eq!((load(&paths).format, load(&paths).thresholds.len()), (FORMAT, 2));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn suffix_per_language() {
        assert_eq!(suffix(10000, "de"), " [ab 10.000]");
        assert_eq!(suffix(10000, "en"), " [10,000+]");
        assert_eq!(with_suffix("Gildenmitglied [ab 800]", " [ab 800]"), "Gildenmitglied [ab 800]");
        assert_eq!(with_suffix("Member [800+]", " [800+]"), "Member [800+]");
        // Like 4.12: a space behind the addition hides it from the pattern.
        assert_eq!(with_suffix("Member [800+] ", " [800+]"), "Member [800+] [800+]");
    }
}
