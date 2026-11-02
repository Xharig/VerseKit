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

//! The graphics renderer of the game (Vulkan or DirectX 11) in `GraphicsSettings.json`
//! (`read_renderer`, `write_renderer`, `renderer_file` of `graphics.py` in 4.12).
//!
//! The file does not lie in the folder of the game but under
//! `%LOCALAPPDATA%/Star Citizen/starcitizen_(sc-alpha-<version>)_<id>/GraphicsSettings/`: one folder to a
//! version of the game, and the one the game touched last is taken. On Linux the folder is searched in every
//! Wine prefix, like the shader cache (`shader_cache::local_appdata_dirs`).
//!
//! The whole file is read and written again as Python's `json.dumps(…, indent=2)` does (keys in the order
//! they had, every sign beyond ASCII as `\u…`), so the game finds it as it left it.

use std::path::Path;
use std::time::SystemTime;

use crate::errors;
use crate::game_files::{self, WriteError};
use crate::jsonio::{self, Value};
use crate::paths::Paths;
use crate::shader_cache;

/// The key in the file.
pub const KEY: &str = "GraphicsRenderer";
/// The two renderers with the key of their text: `(value, text)`.
pub const CHOICES: [(i64, &str); 2] = [(1, "s_gr_renderer_1"), (0, "s_gr_renderer_0")];

/// `GraphicsSettings.json` of the version the game started last, `None` where there is none.
pub fn file(paths: &Paths) -> Option<String> {
    let f = paths.host.flavor;
    let mut candidates: Vec<(SystemTime, String)> = Vec::new();
    for base in shader_cache::local_appdata_dirs(paths) {
        for top in names_of(&base) {
            if top.to_lowercase() != "star citizen" {
                continue;
            }
            let root = f.join_all(&base, &[top.as_str()]);
            for name in names_of(&root) {
                let path = f.join_all(&root, &[name.as_str(), "GraphicsSettings", "GraphicsSettings.json"]);
                if name.to_lowercase().starts_with("starcitizen_")
                    && Path::new(&path).is_file()
                    && let Ok(time) = std::fs::metadata(f.join_all(&root, &[name.as_str()])).and_then(|m| m.modified())
                {
                    candidates.push((time, path));
                }
            }
        }
    }
    candidates.into_iter().max().map(|(_, path)| path)
}

fn names_of(folder: &str) -> Vec<String> {
    std::fs::read_dir(folder)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| e.file_name().to_str().map(str::to_string))
        .collect()
}

/// A value as Python counts it against `(0, 1)`: `true`, `1.0` are 1 too.
fn zero_or_one(value: &Value) -> Option<i64> {
    match value {
        Value::Bool(b) => Some(i64::from(*b)),
        Value::Int(text) => match text.as_str() {
            "0" => Some(0),
            "1" => Some(1),
            _ => None,
        },
        Value::Float(x) if *x == 0.0 => Some(0),
        Value::Float(x) if (*x - 1.0).abs() == 0.0 => Some(1),
        _ => None,
    }
}

/// The renderer in the text of the file: 1 = Vulkan, 0 = DirectX 11, `None` for anything else (no file
/// contents, no such key, another value).
pub fn read_text(text: &str) -> Option<i64> {
    let data = jsonio::loads(text).ok()?;
    zero_or_one(data.get("GraphicsSettings")?.get(KEY)?)
}

/// The renderer in the file (`path`, or the one found); `None` where there is none or it says nothing usable.
pub fn read(paths: &Paths, path: Option<&str>) -> Option<i64> {
    let path = path.map(str::to_string).or_else(|| file(paths))?;
    match std::fs::read(&path) {
        Ok(bytes) => read_text(&String::from_utf8_lossy(&bytes).replace("\r\n", "\n").replace('\r', "\n")),
        Err(error) => {
            errors::record("renderer.read", &error);
            None
        }
    }
}

/// Why the renderer was not set.
#[derive(Debug)]
pub enum Failure {
    /// Not 0 or 1.
    Invalid,
    /// There is no file.
    NoFile,
    /// It could not be read, or it has no `GraphicsSettings` object.
    Read,
    /// The game runs (or nobody can tell) or the copy or the writing failed.
    Write(WriteError),
}

/// Every sign beyond ASCII as `\uXXXX` (a pair for those beyond the plane), as `ensure_ascii=True` writes it.
/// Outside of strings there is no such sign in JSON, so the whole text can be walked.
fn ascii_only(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if (' '..='~').contains(&c) || c == '\n' {
            out.push(c);
        } else {
            let mut units = [0u16; 2];
            for unit in c.encode_utf16(&mut units) {
                out.push_str(&format!("\\u{unit:04x}"));
            }
        }
    }
    out
}

/// The text of the file with the renderer set: `None` where it is that already, the new text otherwise.
///
/// # Errors
///
/// [`Failure::Invalid`] for a value other than 0 or 1, [`Failure::Read`] where the text is no JSON or has no
/// `GraphicsSettings` object.
pub fn change_text(text: &str, value: i64) -> Result<Option<String>, Failure> {
    if !(0..=1).contains(&value) {
        return Err(Failure::Invalid);
    }
    let mut data = jsonio::loads(text).map_err(|_| Failure::Read)?;
    let Value::Object(top) = &mut data else { return Err(Failure::Read) };
    let Some((_, Value::Object(settings))) = top.iter_mut().find(|(k, _)| k == "GraphicsSettings") else {
        return Err(Failure::Read);
    };
    if settings.iter().find(|(k, _)| k == KEY).and_then(|(_, v)| zero_or_one(v)) == Some(value) {
        return Ok(None);
    }
    jsonio::map_set(settings, KEY, Value::int(value));
    Ok(Some(ascii_only(&jsonio::dumps(&data, Some(2), false)) + "\n"))
}

/// Sets the renderer in `GraphicsSettings.json`: a copy first (`game_files`), then the file is written
/// whole with `\n` line ends. Gives the copy, `None` where nothing had to be written. The game must be quiet.
///
/// # Errors
///
/// [`Failure`]: see there.
pub fn write(paths: &Paths, path: Option<&str>, value: i64) -> Result<Option<String>, Failure> {
    let (path, fresh) = prepare(paths, path, value)?;
    let Some(fresh) = fresh else { return Ok(None) };
    game_files::write(paths, &path, fresh.as_bytes()).map_err(Failure::Write)
}

/// Everything [`write`] does except the writing: the file is found and read and the new text worked out, so
/// that a wrong file is known before any other file is touched.
///
/// # Errors
///
/// [`Failure`] as for [`write`], without the writing.
pub fn check(paths: &Paths, path: Option<&str>, value: i64) -> Result<(), Failure> {
    prepare(paths, path, value).map(|_| ())
}

/// The path and the new text (`None`: it is so already).
fn prepare(paths: &Paths, path: Option<&str>, value: i64) -> Result<(String, Option<String>), Failure> {
    let path = path.map(str::to_string).or_else(|| file(paths)).ok_or(Failure::NoFile)?;
    if !(0..=1).contains(&value) {
        return Err(Failure::Invalid);
    }
    // Strict, as 4.12: bytes that are no UTF-8 are not read "as far as they go", writing them back would put
    // replacement signs in their place.
    let text = match std::fs::read(&path)
        .map_err(|e| e.to_string())
        .and_then(|b| String::from_utf8(b).map_err(|e| e.to_string()))
    {
        Ok(text) => text.replace("\r\n", "\n").replace('\r', "\n"),
        Err(error) => {
            errors::record("renderer.write_read", &error);
            return Err(Failure::Read);
        }
    };
    let fresh = change_text(&text, value)?;
    Ok((path, fresh))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game_start;
    use crate::paths::{ENV_INSTALL_DIR, Host};
    use crate::pypath::Flavor;

    const TEXT: &str = "{\n  \"GraphicsSettings\": {\n    \"GraphicsRenderer\": 1,\n    \"Name\": \"Zoë\"\n  }\n}\n";

    #[test]
    fn the_renderer_is_read_where_it_is_zero_or_one_and_nowhere_else() {
        assert_eq!(read_text(TEXT), Some(1));
        assert_eq!(read_text(&TEXT.replace("1,", "0,")), Some(0));
        assert_eq!(read_text(&TEXT.replace("1,", "true,")), Some(1), "Python counts true as 1");
        assert_eq!(read_text(&TEXT.replace("1,", "1.0,")), Some(1));
        assert_eq!(read_text(&TEXT.replace("1,", "2,")), None);
        assert_eq!(read_text(&TEXT.replace("1,", "\"1\",")), None);
        assert_eq!(read_text(&TEXT.replace("GraphicsRenderer", "Other")), None);
        assert_eq!(read_text("not json"), None);
        assert_eq!(read_text("[1]"), None);
    }

    #[test]
    fn a_change_keeps_the_order_writes_two_spaces_and_escapes_what_is_beyond_ascii() {
        let fresh = change_text(TEXT, 0).expect("change").expect("another text");
        assert_eq!(
            fresh,
            "{\n  \"GraphicsSettings\": {\n    \"GraphicsRenderer\": 0,\n    \"Name\": \"Zo\\u00eb\"\n  }\n}\n"
        );
        assert_eq!(change_text(TEXT, 1).expect("same"), None, "already so: nothing to write");
        let beyond = TEXT.replace("Zoë", "😀");
        assert!(change_text(&beyond, 0).unwrap().unwrap().contains("\\ud83d\\ude00"));
        assert!(matches!(change_text(TEXT, 2), Err(Failure::Invalid)));
        assert!(matches!(change_text("{}", 1), Err(Failure::Read)));
        assert!(matches!(change_text("{\"GraphicsSettings\": []}", 1), Err(Failure::Read)));
    }

    #[test]
    fn a_missing_key_is_put_in_at_the_end() {
        let text = "{\"GraphicsSettings\": {\"Other\": 3}}";
        assert_eq!(
            change_text(text, 1).unwrap().unwrap(),
            "{\n  \"GraphicsSettings\": {\n    \"Other\": 3,\n    \"GraphicsRenderer\": 1\n  }\n}\n"
        );
    }

    fn scratch(name: &str) -> String {
        let dir = std::env::temp_dir().join(format!("versekit-renderer-{}-{name}", crate::testdir::run_tag()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("folder");
        dir.to_string_lossy().replace('\\', "/")
    }

    /// A user on Windows' layout: `LOCALAPPDATA` in the throwaway home, a quiet game (log `age` seconds old).
    fn world(name: &str, age: u64) -> (String, Paths) {
        let home = scratch(name);
        let game = format!("{home}/StarCitizen/LIVE");
        std::fs::create_dir_all(&game).expect("folder");
        std::fs::write(format!("{game}/Game.log"), "x").expect("log");
        let at = SystemTime::now() - std::time::Duration::from_secs(age);
        std::fs::File::options()
            .write(true)
            .open(format!("{game}/Game.log"))
            .and_then(|f| f.set_modified(at))
            .expect("age");
        let local = format!("{home}/AppData/Local");
        std::fs::create_dir_all(&local).expect("local");
        let vars = [
            ("SC_BP_HOME", home.as_str()),
            ("HOME", home.as_str()),
            ("LOCALAPPDATA", local.as_str()),
            (ENV_INSTALL_DIR, game.as_str()),
            (game_start::ENV_NO_PROCESS, "1"),
        ];
        let paths = Paths::new(Host::new(Flavor::Windows, &vars, &home));
        (home, paths)
    }

    fn put(home: &str, version: &str, text: &str) -> String {
        let folder = format!("{home}/AppData/Local/Star Citizen/{version}/GraphicsSettings");
        std::fs::create_dir_all(&folder).expect("folder");
        let path = format!("{folder}/GraphicsSettings.json");
        std::fs::write(&path, text).expect("file");
        path
    }

    #[test]
    fn the_folder_the_game_touched_last_is_taken() {
        let (home, paths) = world("neueste", 7200);
        put(&home, "starcitizen_(sc-alpha-4.1.0)_aaaa", TEXT);
        std::thread::sleep(std::time::Duration::from_millis(400));
        put(&home, "StarCitizen_(sc-alpha-4.2.0)_bbbb", TEXT);
        std::thread::sleep(std::time::Duration::from_millis(400));
        // A folder of another name does not count, however new.
        put(&home, "other", TEXT);
        let found = file(&paths).expect("found");
        assert!(found.to_lowercase().replace('\\', "/").contains("4.2.0"), "{found}");
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn writing_keeps_a_copy_and_changes_nothing_while_the_game_runs_or_is_unknown() {
        let (home, paths) = world("schreiben", 7200);
        let path = put(&home, "starcitizen_(sc-alpha-4.2.0)_bbbb", TEXT);
        assert_eq!(read(&paths, None), Some(1));
        let backup = write(&paths, None, 0).expect("written").expect("a copy");
        assert!(backup.contains(".versekit-"), "{backup}");
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "{\n  \"GraphicsSettings\": {\n    \"GraphicsRenderer\": 0,\n    \"Name\": \"Zo\\u00eb\"\n  }\n}\n"
        );
        assert_eq!(std::fs::read_to_string(&backup).unwrap(), TEXT, "the copy is the old state");
        assert!(write(&paths, None, 0).expect("same").is_none(), "no second copy for nothing");
        let _ = std::fs::remove_dir_all(home);
        // The game writes its log right now.
        let (home, paths) = world("laeuft", 5);
        let path = put(&home, "starcitizen_(sc-alpha-4.2.0)_bbbb", TEXT);
        assert!(matches!(write(&paths, None, 0), Err(Failure::Write(WriteError::GameRuns))));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), TEXT);
        let _ = std::fs::remove_dir_all(home);
        // No file at all.
        let (home, paths) = world("keine", 7200);
        assert!(matches!(write(&paths, None, 0), Err(Failure::NoFile)));
        let _ = std::fs::remove_dir_all(home);
        // Bytes that are no UTF-8: not read as far as they go, nothing written.
        let (home, paths) = world("kaputt", 7200);
        let path = put(&home, "starcitizen_(sc-alpha-4.2.0)_bbbb", TEXT);
        let mut bytes = TEXT.as_bytes().to_vec();
        bytes[3] = 0xff;
        std::fs::write(&path, &bytes).unwrap();
        assert!(matches!(write(&paths, None, 0), Err(Failure::Read)));
        assert_eq!(std::fs::read(&path).unwrap(), bytes, "the file is as it was");
        let _ = std::fs::remove_dir_all(home);
    }
}
