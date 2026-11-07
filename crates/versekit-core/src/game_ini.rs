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

//! Which `global.ini` of the game counts, and its lines as Python reads
//! them. Ports `injection.ini_file`, `translation.target_ini`,
//! `translation.game_language` and the parts of `translation` they need.
//! Only reads; nothing here writes to the game.

use std::fs;

use crate::jsonio::{self, Map, Value};
use crate::paths::Paths;
use crate::text::{decode_ignore, py_strip};

/// The note of the installed text sources (`translation.NOTE_FILE`).
pub const NOTE_FILE: &str = "uebersetzung.json";
/// The setting with the chosen text source.
pub const SETTING_SOURCE: &str = "inj_quelle";
/// The setting with the own address per channel (`translation.SETTING_CUSTOM`).
pub const SETTING_CUSTOM: &str = "eigene_quelle";
/// The own address as a source (`translation.CUSTOM`).
pub const CUSTOM: &str = "eigene";
/// The game's own texts as a source.
pub const ORIGINAL: &str = "original";

/// `translation.SOURCES` in their order: name and language folder.
pub const SOURCES: [(&str, &str); 15] = [
    ("starstrings", "english"),
    ("dymerz_de", "german_(germany)"),
    ("dymerz_fr", "french_(france)"),
    ("dymerz_it", "italian_(italy)"),
    ("dymerz_es", "spanish_(spain)"),
    ("dymerz_pt", "portuguese_(brazil)"),
    ("dymerz_tr", "turkish_(turkey)"),
    ("luftwerft_de", "german_(germany)"),
    ("luftwerft_en", "english"),
    ("luftwerft_fr", "french_(france)"),
    ("luftwerft_it", "italian_(italy)"),
    ("luftwerft_es", "spanish_(spain)"),
    ("luftwerft_bar", "bavarian_(germany)"),
    ("luftwerft_nl", "dutch_(netherlands)"),
    ("luftwerft_pt", "portuguese_(portugal)"),
];

/// Language folders of translations that are not among the game's own languages: the folder and `g_language`
/// are named as the source names them.
pub const COMMUNITY_LANGUAGES: [&str; 3] = ["bavarian_(germany)", "dutch_(netherlands)", "portuguese_(portugal)"];

/// Is `language` a language folder a translation may be written to and `g_language` may name: one of the game
/// ([`GAME_LANGUAGES`]) or of a source ([`COMMUNITY_LANGUAGES`]), in any case.
pub fn is_language_folder(language: &str) -> bool {
    let language = language.to_lowercase();
    GAME_LANGUAGES.contains(&language.as_str()) || COMMUNITY_LANGUAGES.contains(&language.as_str())
}

/// The language folders Star Citizen knows (`translation.GAME_LANGUAGES`).
pub const GAME_LANGUAGES: [&str; 11] = [
    "english",
    "german_(germany)",
    "french_(france)",
    "italian_(italy)",
    "spanish_(spain)",
    "portuguese_(brazil)",
    "turkish_(turkey)",
    "chinese_(simplified)",
    "korean_(south_korea)",
    "japanese_(japan)",
    "polish_(poland)",
];

fn is_dir(path: &str) -> bool {
    !path.is_empty() && fs::metadata(path).is_ok_and(|m| m.is_dir())
}

/// `os.path.isfile`.
pub fn is_file(path: &str) -> bool {
    !path.is_empty() && fs::metadata(path).is_ok_and(|m| m.is_file())
}

/// Python's `str.splitlines()` on text read in text mode: `\r\n` and `\r`
/// count as one line end, like `\n`, `\v`, `\f`, `\x1c` to `\x1e`, `\x85`,
/// ` ` and ` `. No empty line after a last line end.
pub fn py_splitlines(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut chars = text.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        let end = match c {
            '\r' => {
                if let Some(&(_, '\n')) = chars.peek() {
                    chars.next();
                }
                true
            }
            '\n' | '\u{0b}' | '\u{0c}' | '\u{1c}' | '\u{1d}' | '\u{1e}' | '\u{85}' | '\u{2028}' | '\u{2029}' => true,
            _ => false,
        };
        if end {
            out.push(&text[start..i]);
            start = chars.peek().map_or(text.len(), |&(j, _)| j);
        }
    }
    if start < text.len() {
        out.push(&text[start..]);
    }
    out
}

/// The lines of a file read with `encoding='utf-8', errors='ignore'`;
/// `None` when it cannot be read.
pub fn read_lines(path: &str) -> Option<Vec<String>> {
    let bytes = fs::read(path).ok()?;
    Some(py_splitlines(&decode_ignore(&bytes)).into_iter().map(str::to_string).collect())
}

/// `translation._known_language`.
fn known_language(language: &str) -> bool {
    is_language_folder(language)
}

/// `translation.target_ini`: where the file of a language folder belongs. An
/// existing `data` or `Data` folder is kept; only for known languages.
pub fn target_ini(paths: &Paths, language: &str) -> Option<String> {
    target_ini_in(paths, &paths.game_folder()?, language)
}

/// `translation.target_ini(sprache, game_dir)` for the folder of a channel.
pub fn target_ini_in(paths: &Paths, root: &str, language: &str) -> Option<String> {
    if !known_language(language) || root.is_empty() {
        return None;
    }
    let root = root.to_string();
    let flavor = paths.host.flavor;
    for spelling in ["data", "Data"] {
        let folder = flavor.join(&root, spelling);
        if is_dir(&folder) {
            return Some(flavor.join_all(&folder, &["Localization", language, "global.ini"]));
        }
    }
    Some(flavor.join_all(&root, &["data", "Localization", language, "global.ini"]))
}

/// `translation.game_language`: the language folder in `g_language` of the
/// `user.cfg`, or `None` when nothing is entered there.
pub fn game_language(paths: &Paths) -> Option<String> {
    game_language_in(paths, &paths.game_folder()?)
}

/// `translation.game_language(game_dir)` for the folder of a channel.
pub fn game_language_in(paths: &Paths, root: &str) -> Option<String> {
    let lines = read_lines(&paths.host.flavor.join(root, "user.cfg"))?;
    for line in &lines {
        let (key, rest) = match line.split_once('=') {
            Some((key, rest)) => (key, Some(rest)),
            None => (line.as_str(), None),
        };
        if py_strip(key) != "g_language" {
            continue;
        }
        let value = py_strip(rest.unwrap_or(""));
        let value = value.split(';').next().unwrap_or("");
        let value = value.split("--").next().unwrap_or("");
        let value = py_strip(value).trim_matches(['"', '\'']);
        if !value.is_empty() {
            return Some(value.to_string());
        }
    }
    None
}

/// `injection._lang_order`: the language of the `user.cfg` first.
fn language_order(paths: &Paths, fallback: &[&str]) -> Vec<String> {
    let mut order: Vec<String> = fallback.iter().map(|s| s.to_string()).collect();
    let Some(language) = game_language(paths) else { return order };
    order.retain(|l| *l != language);
    order.insert(0, language);
    order
}

/// `translation._note`: the note of the installed sources, or empty.
fn note(paths: &Paths) -> Map {
    match jsonio::read_json(&paths.app_file(NOTE_FILE)) {
        Ok(Value::Object(map)) => map,
        _ => Map::new(),
    }
}

fn note_entry<'a>(note: &'a Map, source: &str) -> Option<&'a Value> {
    note.iter().find(|(k, _)| k == source).map(|(_, v)| v)
}

/// Python's truth value.
fn truthy(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null) | Some(Value::Bool(false)) => false,
        Some(Value::Int(n)) => n != "0",
        Some(Value::Float(x)) => *x != 0.0,
        Some(Value::Str(s)) => !s.is_empty(),
        Some(Value::Array(a)) => !a.is_empty(),
        Some(Value::Object(o)) => !o.is_empty(),
        Some(Value::Bool(true)) => true,
    }
}

/// `translation.installed`: a source with an identifier in the note.
fn installed(note: &Map, source: &str) -> bool {
    truthy(note_entry(note, source).and_then(|e| e.get("kennung")))
}

/// `translation.language_folder` for the main installation. A language
/// that is no text counts as none (`target_ini` finds nothing then).
fn language_folder(paths: &Paths, note: &Map, source: &str) -> Option<String> {
    let noted = note_entry(note, source).and_then(|e| e.get("sprache")).filter(|v| truthy(Some(v)));
    if source == CUSTOM {
        if let Some(language) = noted {
            return language.as_str().map(str::to_string);
        }
        let settings = paths.settings();
        let custom = settings.iter().find(|(k, _)| k == SETTING_CUSTOM).and_then(|(_, v)| v.get("MAIN"));
        return match custom.filter(|c| c.as_object().is_some()) {
            Some(custom) if truthy(custom.get("url")) => match custom.get("sprache").filter(|v| truthy(Some(v))) {
                Some(language) => language.as_str().map(str::to_string),
                None => Some("english".into()),
            },
            _ => Some("english".into()),
        };
    }
    SOURCES.iter().find(|(name, _)| *name == source).map(|(_, language)| language.to_string())
}

/// `injection.ini_file`: the `global.ini` that counts. The chosen text
/// source first, then the installed ones, then the file that is there, in
/// the order the game uses.
pub fn ini_file(paths: &Paths) -> Option<String> {
    ini_choice(paths).path
}

/// The `global.ini` that counts, with its language folder and the source
/// that placed it (`injection.ini_file` returns all three).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IniChoice {
    pub path: Option<String>,
    pub language: String,
    pub source: Option<String>,
}

/// `injection.ini_file` with folder and source.
pub fn ini_choice(paths: &Paths) -> IniChoice {
    let chosen = paths.setting(SETTING_SOURCE);
    let mut order: Vec<&str> = SOURCES.iter().map(|(name, _)| *name).chain([CUSTOM]).collect();
    match chosen.as_deref() {
        Some(source) if order.contains(&source) => {
            order.retain(|s| *s != source);
            order.insert(0, source);
        }
        Some(ORIGINAL) => {
            for language in language_order(paths, &["english", "german_(germany)"]) {
                if let Some(path) = target_ini(paths, &language)
                    && is_file(&path)
                {
                    return IniChoice { path: Some(path), language, source: None };
                }
            }
        }
        _ => {}
    }
    let note = note(paths);
    for source in order {
        if installed(&note, source) {
            let language = language_folder(paths, &note, source);
            return IniChoice {
                path: language.as_deref().and_then(|language| target_ini(paths, language)),
                language: language.unwrap_or_default(),
                source: Some(source.to_string()),
            };
        }
    }
    for language in language_order(paths, &["german_(germany)", "english"]) {
        if let Some(path) = target_ini(paths, &language)
            && is_file(&path)
        {
            return IniChoice { path: Some(path), language, source: None };
        }
    }
    IniChoice { path: None, language: "english".into(), source: None }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splitlines_like_python() {
        assert_eq!(py_splitlines("a\r\nb\rc\nd"), vec!["a", "b", "c", "d"]);
        assert_eq!(py_splitlines("a\n"), vec!["a"]);
        assert_eq!(py_splitlines("\n"), vec![""]);
        assert_eq!(py_splitlines(""), Vec::<&str>::new());
        assert_eq!(py_splitlines("a\u{2028}b\u{1c}c\u{85}"), vec!["a", "b", "c"]);
        assert_eq!(py_splitlines("a\r\r\nb"), vec!["a", "", "b"]);
    }

    #[test]
    fn known_languages_ignore_case() {
        assert!(known_language("German_(Germany)"));
        assert!(!known_language("klingon"));
        for language in COMMUNITY_LANGUAGES {
            assert!(known_language(language) && known_language(&language.to_uppercase()), "{language}");
            assert!(!GAME_LANGUAGES.contains(&language), "{language} is no language of the game");
        }
    }
}
