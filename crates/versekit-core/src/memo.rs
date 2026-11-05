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

//! The memo pages ("Notizen", new in v5): free text the player jots down,
//! one memo per page, kept in `notizen.json` in the data folder as
//! `{"format": 1, "zettel": [{"text", "bearbeitet", "formatierung"?}]}`.
//!
//! A memo is named by its first line that holds something. The list keeps
//! its order (a new memo stands on top), so that it does not jump while one
//! types; "last edited" is told by the time each memo carries. The look of
//! the text (bold, lists, …) stands beside the plain text, see [`format`];
//! a memo without one is written exactly as before.

pub mod format;
pub mod table;

use std::sync::Mutex;

use crate::jsonio::{self, Map, Value};
use crate::paths::Paths;
use format::Formatting;

/// The key of the look in a memo's entry.
const LOOK_KEY: &str = "formatierung";

/// The file of the memos (in the folder `Intern`).
pub const FILE: &str = "notizen.json";
/// The format written.
const FORMAT: i64 = 1;

/// One writer of the file at a time.
static LOCK: Mutex<()> = Mutex::new(());

/// One memo: its text, when it was last changed (seconds since 1970), and the look of its text.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Memo {
    pub text: String,
    pub edited: i64,
    pub look: Formatting,
}

impl Memo {
    /// The first line that holds something, without the blanks around it; empty for a memo without text.
    pub fn title(&self) -> &str {
        self.text.lines().map(str::trim).find(|line| !line.is_empty()).unwrap_or("")
    }

    /// The `most` lines after the title that hold something, without the blanks around them.
    pub fn following(&self, most: usize) -> Vec<&str> {
        self.text.lines().map(str::trim).filter(|line| !line.is_empty()).skip(1).take(most).collect()
    }
}

/// All memos, the top one first.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Memos {
    pub items: Vec<Memo>,
}

impl Memos {
    /// An empty memo on top, edited at `now`. Returns its place.
    pub fn add(&mut self, now: i64) -> usize {
        self.items.insert(0, Memo { text: String::new(), edited: now, look: Formatting::default() });
        0
    }

    /// The memo at `at` and removes it from the list.
    pub fn remove(&mut self, at: usize) -> Option<Memo> {
        (at < self.items.len()).then(|| self.items.remove(at))
    }

    /// A memo back at the place where it stood (at the end where the list got shorter).
    pub fn restore(&mut self, at: usize, memo: Memo) -> usize {
        let at = at.min(self.items.len());
        self.items.insert(at, memo);
        at
    }

    /// Sets the text of the memo at `at` and stamps it as edited at `now`; false where nothing changed. The
    /// look follows the change (see [`Formatting::follow_edit`]).
    pub fn set_text(&mut self, at: usize, text: &str, now: i64) -> bool {
        self.edit(at, text, None, None, now)
    }

    /// [`Memos::set_text`] with where the cursor stands after the change and the styles of new letters
    /// where they were chosen before typing.
    pub fn edit(&mut self, at: usize, text: &str, cursor: Option<usize>, typing: Option<u8>, now: i64) -> bool {
        match self.items.get_mut(at) {
            Some(memo) if memo.text != text => {
                memo.look.follow_edit(&memo.text, text, cursor, typing);
                memo.text = text.to_string();
                memo.edited = now;
                true
            }
            _ => false,
        }
    }

    /// Sets the text and the look of the memo at `at` together (a change of a table) and stamps it as edited
    /// at `now`; false where nothing changed.
    pub fn rebuild(&mut self, at: usize, text: String, look: Formatting, now: i64) -> bool {
        match self.items.get_mut(at) {
            Some(memo) if memo.text != text || memo.look != look => {
                (memo.text, memo.look, memo.edited) = (text, look, now);
                true
            }
            _ => false,
        }
    }

    /// Sets the look of the memo at `at` and stamps it as edited at `now`; false where nothing changed.
    pub fn set_look(&mut self, at: usize, look: Formatting, now: i64) -> bool {
        match self.items.get_mut(at) {
            Some(memo) if memo.look != look => {
                memo.look = look;
                memo.edited = now;
                true
            }
            _ => false,
        }
    }

    /// The memo edited last; the upper one where two share the time.
    pub fn latest(&self) -> Option<&Memo> {
        self.items.iter().reduce(|best, memo| if memo.edited > best.edited { memo } else { best })
    }
}

fn field<'a>(map: &'a Map, key: &str) -> Option<&'a Value> {
    map.iter().find(|(k, _)| k == key).map(|(_, v)| v)
}

/// The memos in the file; none without one or with one that cannot be read.
pub fn load(paths: &Paths) -> Memos {
    let Ok(Value::Object(data)) = jsonio::read_json(&paths.app_file(FILE)) else { return Memos::default() };
    let Some(Value::Array(list)) = field(&data, "zettel") else { return Memos::default() };
    let items = list
        .iter()
        .filter_map(Value::as_object)
        .map(|entry| {
            let text = field(entry, "text").and_then(Value::as_str).unwrap_or("").to_string();
            Memo {
                look: Formatting::from_value(field(entry, LOOK_KEY), &text),
                text,
                edited: match field(entry, "bearbeitet") {
                    Some(Value::Int(n)) => n.parse().unwrap_or(0),
                    // A time written as a fraction counts to the second.
                    Some(Value::Float(f)) => *f as i64,
                    _ => 0,
                },
            }
        })
        .collect();
    Memos { items }
}

/// Writes the memos, whole and at once.
///
/// # Errors
///
/// When the file cannot be written.
pub fn save(paths: &Paths, memos: &Memos) -> std::io::Result<()> {
    let _guard = LOCK.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let list = memos
        .items
        .iter()
        .map(|memo| {
            let mut entry: Map = vec![
                ("text".into(), Value::str(memo.text.as_str())),
                ("bearbeitet".into(), Value::int(memo.edited)),
            ];
            if let Some(look) = memo.look.to_value(&memo.text) {
                entry.push((LOOK_KEY.into(), look));
            }
            Value::Object(entry)
        })
        .collect();
    let data = Value::Object(vec![("format".into(), Value::int(FORMAT)), ("zettel".into(), Value::Array(list))]);
    let target = paths.app_file(FILE);
    let folder = paths.host.flavor.dirname(&target);
    if !folder.is_empty() {
        jsonio::make_dirs(&folder)?;
    }
    jsonio::write_text_atomic(&target, &jsonio::dumps(&data, Some(1), false), jsonio::Newline::Keep)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths_in(name: &str) -> (Paths, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("versekit-notizen-{}-{name}", crate::testdir::run_tag()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let home = dir.to_string_lossy().into_owned();
        let vars = [(crate::legacy::ENV_HOME, home.as_str()), (crate::legacy::ENV_NO_NET, "1")];
        (Paths::new(crate::paths::Host::new(crate::pypath::Flavor::native(), &vars, &home)), dir)
    }

    #[test]
    fn a_memo_is_named_by_its_first_line_with_something_in_it() {
        let memo = Memo { text: "\n  \n  Titanium holen  \n\nLorville\nArea18\r\n".into(), edited: 0, ..Default::default() };
        assert_eq!(memo.title(), "Titanium holen");
        assert_eq!(memo.following(5), ["Lorville", "Area18"]);
        assert_eq!(memo.following(1), ["Lorville"]);
        let empty = Memo { text: " \n\n ".into(), edited: 0, ..Default::default() };
        assert_eq!((empty.title(), empty.following(3).len()), ("", 0));
    }

    #[test]
    fn a_new_memo_stands_on_top_and_a_deleted_one_comes_back_to_its_place() {
        let mut memos = Memos::default();
        let first = memos.add(10);
        memos.set_text(first, "eins", 11);
        let second = memos.add(20);
        memos.set_text(second, "zwei", 21);
        memos.add(30);
        memos.set_text(0, "drei", 31);
        let texts = |memos: &Memos| memos.items.iter().map(|m| m.text.clone()).collect::<Vec<_>>();
        assert_eq!(texts(&memos), ["drei", "zwei", "eins"]);
        let gone = memos.remove(1).unwrap();
        assert_eq!(texts(&memos), ["drei", "eins"]);
        assert_eq!(memos.restore(1, gone), 1);
        assert_eq!(texts(&memos), ["drei", "zwei", "eins"]);
        assert!(memos.remove(7).is_none());
        assert_eq!(memos.restore(9, Memo::default()), 3, "at the end where the list got shorter");
    }

    #[test]
    fn the_time_changes_with_the_text_only() {
        let mut memos = Memos::default();
        memos.add(10);
        assert!(memos.set_text(0, "a", 20));
        assert!(!memos.set_text(0, "a", 99), "the same text is no edit");
        assert!(!memos.set_text(5, "a", 99), "no such memo");
        assert_eq!(memos.items[0].edited, 20);
    }

    #[test]
    fn the_latest_is_the_one_edited_last_and_the_upper_one_on_a_tie() {
        let mut memos = Memos::default();
        assert!(memos.latest().is_none());
        for (text, edited) in [("alt", 5), ("neu", 50), ("auch neu", 50)] {
            memos.items.push(Memo { text: text.into(), edited, ..Default::default() });
        }
        assert_eq!(memos.latest().map(|m| m.text.as_str()), Some("neu"));
    }

    #[test]
    fn the_memos_are_kept_in_their_file_and_a_broken_file_reads_as_none() {
        let (paths, dir) = paths_in("datei");
        assert_eq!(load(&paths), Memos::default());
        let mut memos = Memos::default();
        memos.add(100);
        memos.set_text(0, "Größe prüfen\n\"Quoten\" und \\ und 🚀\n", 200);
        memos.add(300);
        save(&paths, &memos).unwrap();
        assert_eq!(load(&paths), memos);
        std::fs::write(paths.app_file(FILE), "{ not json").unwrap();
        assert_eq!(load(&paths), Memos::default());
        std::fs::write(
            paths.app_file(FILE),
            r#"{"format": 1, "zettel": [{"text": "a", "bearbeitet": 2.9}, 7, {"bearbeitet": 4}, {"text": 5}]}"#,
        )
        .unwrap();
        let read = load(&paths);
        assert_eq!(
            read.items.iter().map(|m| (m.text.as_str(), m.edited)).collect::<Vec<_>>(),
            [("a", 2), ("", 4), ("", 0)],
            "an entry that is no object goes, a missing field is empty"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A file as 5.1.0 writes it opens with every memo, its text unchanged and no look, and is written again
    /// byte for byte the same.
    #[test]
    fn a_file_of_the_version_before_the_look_opens_unchanged_and_is_written_the_same() {
        let (paths, dir) = paths_in("alt");
        let old = "{\n \"format\": 1,\n \"zettel\": [\n  {\n   \"text\": \"Titanium holen\\nLorville\\n- kein Punkt\",\n   \"bearbeitet\": 1762300000\n  },\n  {\n   \"text\": \"**nicht fett**\",\n   \"bearbeitet\": 1762300100\n  }\n ]\n}";
        std::fs::create_dir_all(std::path::Path::new(&paths.app_file(FILE)).parent().unwrap()).unwrap();
        std::fs::write(paths.app_file(FILE), old).unwrap();
        let memos = load(&paths);
        assert_eq!(memos.items.len(), 2);
        assert_eq!(memos.items[0].text, "Titanium holen\nLorville\n- kein Punkt");
        assert_eq!(memos.items[1].text, "**nicht fett**", "signs in the text stay signs");
        assert!(memos.items.iter().all(|m| m.look.is_plain()));
        save(&paths, &memos).unwrap();
        assert_eq!(std::fs::read_to_string(paths.app_file(FILE)).unwrap(), old);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A memo with a look keeps its text plain in the file (as the version before reads it) and the look
    /// beside it, and reads back the same.
    #[test]
    fn the_look_is_kept_beside_the_plain_text() {
        let (paths, dir) = paths_in("aussehen");
        let mut memos = Memos::default();
        memos.add(1);
        memos.set_text(0, "Einkauf\nTitanium\nGold", 2);
        let mut look = memos.items[0].look.clone();
        look.set_style(8..16, format::BOLD, true);
        look.set_lines(1..3, format::LineKind::Bullet);
        assert!(memos.set_look(0, look.clone(), 3));
        assert!(!memos.set_look(0, look, 4), "the same look is no edit");
        assert_eq!(memos.items[0].edited, 3);
        save(&paths, &memos).unwrap();
        let Ok(Value::Object(data)) = jsonio::read_json(&paths.app_file(FILE)) else { panic!("written") };
        let Some(Value::Array(list)) = field(&data, "zettel") else { panic!("memos") };
        let entry = list[0].as_object().unwrap();
        assert_eq!(field(entry, "text").and_then(Value::as_str), Some("Einkauf\nTitanium\nGold"));
        assert!(field(entry, LOOK_KEY).is_some());
        assert_eq!(load(&paths), memos);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_locked_folder_is_an_error_and_leaves_no_file() {
        let (paths, dir) = paths_in("gesperrt");
        crate::writeguard::deny(true);
        let written = save(&paths, &Memos { items: vec![Memo { text: "x".into(), edited: 1, ..Default::default() }] });
        crate::writeguard::deny(false);
        assert!(written.is_err());
        assert!(!std::path::Path::new(&paths.app_file(FILE)).exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
