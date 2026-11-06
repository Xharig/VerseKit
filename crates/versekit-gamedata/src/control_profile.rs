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

//! The `defaultProfile.xml` of the game: what the factory setting binds and how the actions are called
//! (`_profile` of `joysticks.py`). It lies in the `Data.p4k` as CryXmlB and is read from there, read only.
//!
//! ⚠ Opening the archive takes noticeably long, and the data change only with a patch of the game: the result is
//! kept in `aktionsnamen.json` in the data folder of the program (the file of 4.12, same shape) and read again
//! only when the archive is another one. The mark of the archive is path, size and time in nanoseconds: the bare
//! time does not suffice, an archive can change without being written (two installations side by side, a
//! backup put back, a copy with the date kept).

use std::path::Path;
use std::time::UNIX_EPOCH;

use versekit_core::controls::Profile;
use versekit_core::errors;
use versekit_core::jsonio::{self, Value};
use versekit_core::paths::Paths;

use crate::{Error, GameData};

/// The entry in the archive.
pub const ENTRY: &str = "Data/Libs/Config/defaultProfile.xml";
/// The file in the data folder of the program.
pub const CACHE_FILE: &str = "aktionsnamen.json";
/// Changes when what is kept changes, or the way the mark is made: older files then count for nothing.
pub const CACHE_VERSION: i64 = 4;

/// The `defaultProfile.xml` of the archive as XML text.
///
/// # Errors
///
/// [`Error::MissingEntry`] when there is none, [`Error::Encrypted`], [`Error::Read`] when it is no CryXml.
pub fn profile_xml(game: &GameData) -> Result<String, Error> {
    let bytes = game.entry_bytes(ENTRY)?;
    let xml = p4kx_cryxml::read(&bytes).map_err(|source| Error::Read {
        entry: ENTRY.to_string(),
        source: std::io::Error::new(std::io::ErrorKind::InvalidData, source.to_string()),
    })?;
    let mut out = Vec::new();
    xml.write_to(&mut out).map_err(|source| Error::Read { entry: ENTRY.to_string(), source })?;
    Ok(String::from_utf8_lossy(&out).into_owned())
}

/// The mark of an archive: `path:size:time in nanoseconds`, the path resolved (no `\\?\` in front).
pub fn mark(archive: &Path) -> String {
    let Ok(real) = std::fs::canonicalize(archive) else { return String::new() };
    let Ok(info) = std::fs::metadata(&real) else { return String::new() };
    let nanos = info.modified().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map_or(0, |d| d.as_nanos());
    let text = real.to_string_lossy();
    let text = text.strip_prefix(r"\\?\").unwrap_or(&text);
    format!("{text}:{}:{nanos}", info.len())
}

fn to_value(profile: &Profile, mark: &str) -> Value {
    let labels = profile
        .labels
        .iter()
        .map(|(k, (l, d))| (k.to_string(), Value::Array(vec![Value::str(l.as_str()), Value::str(d.as_str())])))
        .collect();
    let standard = profile
        .defaults
        .iter()
        .map(|(k, fields)| {
            (k.to_string(), Value::Object(fields.iter().map(|(f, i)| (f.clone(), Value::str(i.as_str()))).collect()))
        })
        .collect();
    let groups = profile.groups.iter().map(|(k, g)| (k.to_string(), Value::str(g.as_str()))).collect();
    Value::Object(vec![
        ("fassung".into(), Value::int(CACHE_VERSION)),
        ("stand".into(), Value::str(mark)),
        ("etiketten".into(), Value::Object(labels)),
        ("standard".into(), Value::Object(standard)),
        ("gruppen".into(), Value::Object(groups)),
    ])
}

/// What a kept file says, if it is of this version and of this archive and holds labels.
fn from_value(value: &Value, mark: &str) -> Option<Profile> {
    if value.get("fassung") != Some(&Value::int(CACHE_VERSION))
        || value.get("stand").and_then(Value::as_str) != Some(mark)
    {
        return None;
    }
    let mut profile = Profile::default();
    for (action, pair) in value.get("etiketten")?.as_object()? {
        let Value::Array(pair) = pair else { continue };
        let text = |i: usize| pair.get(i).and_then(Value::as_str).unwrap_or_default().to_string();
        profile.labels.set(action, (text(0), text(1)));
    }
    if profile.labels.is_empty() {
        return None;
    }
    for (action, fields) in value.get("standard")?.as_object()? {
        let Some(fields) = fields.as_object() else { continue };
        let list: Vec<(String, String)> =
            fields.iter().filter_map(|(f, i)| i.as_str().map(|i| (f.clone(), i.to_string()))).collect();
        profile.defaults.set(action, list);
    }
    for (action, group) in value.get("gruppen")?.as_object()? {
        if let Some(group) = group.as_str() {
            profile.groups.set(action, group.to_string());
        }
    }
    Some(profile)
}

/// The profile of the game of these paths: from the kept file when the archive is the same, else from the
/// archive (and kept). Without an archive, or where it can not be read, an empty profile: the page shows the
/// technical names and only the own changes, worse but not broken.
pub fn profile(paths: &Paths) -> Profile {
    let Some(archive) = paths.archive_file() else { return Profile::default() };
    let mark = mark(Path::new(&archive));
    let cache = paths.app_file(CACHE_FILE);
    if !mark.is_empty()
        && let Some(value) = jsonio::load_json(&cache, None, "control_profile.cache")
        && let Some(kept) = from_value(&value, &mark)
    {
        return kept;
    }
    let _topic = versekit_core::archive_lock::reading_for("controls");
    let loaded = GameData::open_file(&archive).and_then(|game| profile_xml(&game));
    let xml = match loaded {
        Ok(xml) => xml,
        Err(error) => {
            errors::record("control_profile.read", &error);
            return Profile::default();
        }
    };
    let Some(profile) = Profile::from_xml(&xml) else {
        errors::record("control_profile.parse", &"the defaultProfile.xml is not well formed");
        return Profile::default();
    };
    if !mark.is_empty() && !profile.is_empty() {
        let _ = jsonio::save_json(&cache, &to_value(&profile, &mark), 2, false);
    }
    profile
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_profile_survives_the_kept_file_and_a_foreign_mark_is_not_taken() {
        let profile = Profile::from_xml(
            r#"<p><actionmap name="g"><action name="a" UILabel="@l" UIDescription="@d" keyboard="f" joystick="button1"/></actionmap></p>"#,
        )
        .expect("profile");
        let value = to_value(&profile, "mark:1:2");
        let text = jsonio::dumps(&value, Some(2), false);
        let back = from_value(&jsonio::loads(&text).expect("json"), "mark:1:2").expect("kept");
        assert_eq!(back.labels.get("a"), Some(&("@l".to_string(), "@d".to_string())));
        assert_eq!(back.defaults.get("a").map(Vec::len), Some(2));
        assert_eq!(back.groups.get("a").map(String::as_str), Some("g"));
        assert!(from_value(&value, "other:1:2").is_none(), "another archive");
        assert!(from_value(&Value::Null, "mark:1:2").is_none());
    }

    /// Reads (only reads) the `Data.p4k` of the game installed on this computer; run by hand with `--ignored`.
    #[test]
    #[ignore = "reads the Data.p4k of the installed game"]
    fn the_installed_game_gives_a_profile() {
        let game = GameData::open_installed().expect("a game installed");
        let xml = profile_xml(&game).expect("the entry");
        let profile = Profile::from_xml(&xml).expect("well formed");
        println!(
            "labels {} defaults {} groups {} (4.12 measured 646 named actions, 411 of them unbound)",
            profile.labels.len(),
            profile.defaults.len(),
            profile.groups.len()
        );
        assert!(profile.labels.len() > 300 && !profile.defaults.is_empty() && !profile.groups.is_empty());
        // For the comparison with 4.12 (`tools/parity_belegung_echt.py`): the same shape as its kept file.
        if let Ok(target) = std::env::var("VERSEKIT_PROFILE_OUT") {
            std::fs::write(target, jsonio::dumps(&to_value(&profile, "x"), Some(1), false)).expect("write");
        }
    }

    #[test]
    fn a_kept_file_of_another_version_is_not_taken() {
        let profile =
            Profile::from_xml(r#"<p><actionmap name="g"><action name="a" UILabel="@l" keyboard="f"/></actionmap></p>"#)
                .expect("profile");
        let mut value = to_value(&profile, "mark:1:2");
        assert!(from_value(&value, "mark:1:2").is_some(), "the same version is taken");
        if let Value::Object(fields) = &mut value {
            for (name, field) in fields.iter_mut() {
                if name == "fassung" {
                    *field = Value::int(CACHE_VERSION - 1);
                }
            }
        }
        assert!(from_value(&value, "mark:1:2").is_none(), "an older version counts for nothing");
    }

    #[test]
    fn a_missing_archive_gives_an_empty_profile() {
        let home = std::env::temp_dir().join(format!("versekit-controlprofile-{}", std::process::id()));
        // A game folder of its own without an archive: the test must not find the installation of this computer.
        let game = home.join("LIVE");
        std::fs::create_dir_all(&game).expect("folder");
        std::fs::write(game.join("Game.log"), "").expect("log");
        let (text, game) = (home.to_string_lossy().into_owned(), game.to_string_lossy().into_owned());
        let vars = [
            ("SC_BP_HOME", text.as_str()),
            ("HOME", text.as_str()),
            (versekit_core::paths::ENV_INSTALL_DIR, game.as_str()),
        ];
        let paths = Paths::new(versekit_core::paths::Host::new(versekit_core::pypath::Flavor::native(), &vars, &text));
        assert!(profile(&paths).is_empty());
        let _ = std::fs::remove_dir_all(home);
    }
}
