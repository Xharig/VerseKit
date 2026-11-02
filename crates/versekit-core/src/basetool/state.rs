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

//! The sync state of one installation, `basetool-<installationId>.json` in
//! the data folder (`basetool_sync.load_state`, `save_state`,
//! `forget_installation`).
//!
//! ⚠ One file per installation, named after its `installationId`: with a
//! data folder that Windows and Linux share, each installation has its own
//! file and none reads the other's. Fields this version does not know stay
//! as they are.

use crate::jsonio::{self, Map, Value, map_set};
use crate::paths::Paths;

/// `basetool-<id>.json`; characters outside `A-Za-z0-9_.-` become `_`.
pub fn file_name(installation_id: &str) -> String {
    let id = if installation_id.is_empty() { "unbekannt" } else { installation_id };
    let safe: String =
        id.chars().map(|c| if c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-') { c } else { '_' }).collect();
    format!("basetool-{safe}.json")
}

pub fn state_file(paths: &Paths, installation_id: &str) -> String {
    paths.app_file(&file_name(installation_id))
}

/// A fresh state, the fields in the order of the previous version.
pub fn empty_state(installation_id: &str) -> Map {
    let object = || Value::Object(Map::new());
    let list = || Value::Array(Vec::new());
    vec![
        ("format".into(), Value::int(1)),
        ("installation_id".into(), Value::str(installation_id)),
        ("cursor".into(), Value::Null),
        ("baseline".into(), object()),
        ("links".into(), object()),
        ("resolved".into(), object()),
        ("conflicts".into(), object()),
        ("pending_removals".into(), list()),
        ("overrides".into(), list()),
        ("own_removed_keys".into(), list()),
        ("local_removed".into(), object()),
        ("accounts".into(), object()),
        ("label_sent".into(), Value::Null),
    ]
}

/// The state from the file; anything unreadable or of another format gives
/// a fresh one. Read fields go over the defaults (`dict.update`).
pub fn load_state(paths: &Paths, installation_id: &str) -> Map {
    let mut state = empty_state(installation_id);
    let Ok(Value::Object(data)) = jsonio::read_json(&state_file(paths, installation_id)) else { return state };
    let format = data.iter().find(|(k, _)| k == "format").map(|(_, v)| v);
    if !jsonio::py_eq(format, Some(&Value::int(1))) {
        return state;
    }
    for (key, value) in data {
        map_set(&mut state, &key, value);
    }
    state
}

/// Like [`load_state`], but `None` when the file is there and cannot be read
/// now (locked, a folder): whoever changes the state then must not write an
/// empty one over it. A missing file or one of another format gives a fresh state.
pub fn try_load(paths: &Paths, installation_id: &str) -> Option<Map> {
    match jsonio::read_json(&state_file(paths, installation_id)) {
        Err(jsonio::ReadError::Os(_)) => None,
        _ => Some(load_state(paths, installation_id)),
    }
}

/// Writes the state atomically with a previous version beside it.
pub fn save_state(paths: &Paths, state: &Map) -> std::io::Result<()> {
    let id = state.iter().find(|(k, _)| k == "installation_id").and_then(|(_, v)| v.as_str()).unwrap_or_default();
    jsonio::save_json(&state_file(paths, id), &Value::Object(state.clone()), 1, false)
}

/// After disconnecting the state is worthless: the file and its previous
/// version go.
pub fn forget_installation(paths: &Paths, installation_id: &str) {
    let file = state_file(paths, installation_id);
    let backup = format!("{}.bak.json", file.strip_suffix(".json").unwrap_or(&file));
    for target in [file, backup] {
        let _ = jsonio::remove_file(&target);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::Host;

    fn paths(name: &str) -> (Paths, String) {
        let dir = std::env::temp_dir().join(format!("versekit-state-{name}-{}", crate::testdir::run_tag()));
        let _ = std::fs::remove_dir_all(&dir);
        let dir = dir.to_string_lossy().into_owned();
        let host = Host::new(crate::pypath::Flavor::native(), &[(crate::legacy::ENV_HOME, &dir)], &dir);
        (Paths::new(host), dir)
    }

    #[test]
    fn names_are_safe() {
        assert_eq!(file_name("inst-1.a_b"), "basetool-inst-1.a_b.json");
        assert_eq!(file_name("../ä x"), "basetool-..___x.json");
        assert_eq!(file_name(""), "basetool-unbekannt.json");
    }

    /// A state that cannot be read now is not an empty state: a decision
    /// must not write over it. A missing one or one of another format is fresh.
    #[test]
    fn an_unreadable_state_is_not_replaced() {
        let (paths, dir) = paths("unlesbar");
        assert_eq!(try_load(&paths, "inst-9"), Some(empty_state("inst-9")), "missing: fresh");
        std::fs::create_dir_all(state_file(&paths, "inst-9")).unwrap();
        assert_eq!(try_load(&paths, "inst-9"), None, "a folder where the file should be");
        assert_eq!(load_state(&paths, "inst-9"), empty_state("inst-9"), "the plain load stays as it was");
        std::fs::remove_dir_all(state_file(&paths, "inst-9")).unwrap();
        std::fs::write(state_file(&paths, "inst-9"), r#"{"format": 2}"#).unwrap();
        assert_eq!(try_load(&paths, "inst-9"), Some(empty_state("inst-9")));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn round_trip_keeps_unknown_fields() {
        let (paths, dir) = paths("rt");
        let mut state = load_state(&paths, "inst-1");
        assert_eq!(state, empty_state("inst-1"));
        map_set(&mut state, "label_sent", Value::str("VerseKit Linux"));
        map_set(&mut state, "neu", Value::Array(vec![Value::Float(0.1)]));
        save_state(&paths, &state).unwrap();
        assert_eq!(load_state(&paths, "inst-1"), state);
        std::fs::write(state_file(&paths, "inst-1"), r#"{"format": 2, "cursor": "x"}"#).unwrap();
        assert_eq!(load_state(&paths, "inst-1"), empty_state("inst-1"));
        forget_installation(&paths, "inst-1");
        assert!(!std::path::Path::new(&state_file(&paths, "inst-1")).exists());
        let _ = std::fs::remove_dir_all(dir);
    }
}
