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

//! The relations of the player: a handle or an organization marked friendly, neutral or hostile, so that the
//! search and the log monitor colour it. Kept only on this computer (`beziehungen.json` in `Intern`); the same
//! text goes out and comes in as a file, so that players can hand their lists on.
//!
//! A mark on the handle counts before a mark on one of its organizations; of those, the main one first.

use crate::handle_query::page::valid_handle;
use crate::jsonio::{self, Value};
use crate::paths::Paths;

/// The file of the relations.
pub const FILE: &str = "beziehungen.json";
/// The mark of the form, in the file and in a list handed on.
pub const FORMAT: &str = "versekit-beziehungen";
const VERSION: i64 = 1;
/// The most relations kept.
pub const MAX: usize = 5000;

/// How the player stands to a handle or an organization.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Stance {
    Friendly,
    Neutral,
    Hostile,
}

impl Stance {
    pub const ALL: [Stance; 3] = [Stance::Friendly, Stance::Neutral, Stance::Hostile];

    /// The word in the file.
    pub fn word(self) -> &'static str {
        match self {
            Stance::Friendly => "freundlich",
            Stance::Neutral => "neutral",
            Stance::Hostile => "feindlich",
        }
    }

    /// The stance of a word in the file; the English words are understood too.
    pub fn from_word(word: &str) -> Option<Stance> {
        match word.trim().to_lowercase().as_str() {
            "freundlich" | "friendly" => Some(Stance::Friendly),
            "neutral" => Some(Stance::Neutral),
            "feindlich" | "hostile" => Some(Stance::Hostile),
            _ => None,
        }
    }

    /// The text key of its name.
    pub fn label(self) -> &'static str {
        match self {
            Stance::Friendly => "s_hq_freundlich",
            Stance::Neutral => "s_hq_neutral",
            Stance::Hostile => "s_hq_feindlich",
        }
    }
}

/// What a relation is about.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    Handle,
    Org,
}

impl Kind {
    fn word(self) -> &'static str {
        match self {
            Kind::Handle => "handle",
            Kind::Org => "org",
        }
    }

    fn from_word(word: &str) -> Option<Kind> {
        match word.trim().to_lowercase().as_str() {
            "handle" => Some(Kind::Handle),
            "org" => Some(Kind::Org),
            _ => None,
        }
    }
}

/// One relation: a handle or the SID of an organization, and the stance.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Relation {
    pub kind: Kind,
    pub name: String,
    pub stance: Stance,
}

/// Why a list handed on was not taken.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImportError {
    /// Not JSON at all.
    NotJson,
    /// JSON, but not a list of relations.
    WrongForm,
}

/// The relations of the player.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Relations {
    list: Vec<Relation>,
}

impl Relations {
    /// The relations kept; none when the file is missing or broken.
    pub fn load(paths: &Paths) -> Relations {
        let valid = |value: &Value| value.get("format").and_then(Value::as_str) == Some(FORMAT);
        jsonio::load_json(&paths.app_file(FILE), Some(&valid), "beziehungen")
            .map(|data| Relations::from_value(&data).unwrap_or_default())
            .unwrap_or_default()
    }

    /// Writes them into the data folder. True when that worked.
    pub fn save(&self, paths: &Paths) -> bool {
        jsonio::save_json(&paths.app_file(FILE), &self.to_value(), 1, false).is_ok()
    }

    pub fn list(&self) -> &[Relation] {
        &self.list
    }

    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    /// The stance set for `name`; the site does not tell capitals from small letters.
    pub fn of(&self, kind: Kind, name: &str) -> Option<Stance> {
        self.list.iter().find(|r| r.kind == kind && r.name.eq_ignore_ascii_case(name)).map(|r| r.stance)
    }

    /// Sets the stance for `name`, or takes it away with `None`. True when something changed; a name no handle
    /// or SID can have changes nothing.
    pub fn set(&mut self, kind: Kind, name: &str, stance: Option<Stance>) -> bool {
        let Some(name) = valid_handle(name) else { return false };
        let at = self.list.iter().position(|r| r.kind == kind && r.name.eq_ignore_ascii_case(&name));
        match (at, stance) {
            (Some(i), Some(stance)) if self.list[i].stance == stance && self.list[i].name == name => false,
            (Some(i), Some(stance)) => {
                self.list[i] = Relation { kind, name, stance };
                true
            }
            (Some(i), None) => {
                self.list.remove(i);
                true
            }
            (None, Some(stance)) if self.list.len() < MAX => {
                self.list.push(Relation { kind, name, stance });
                true
            }
            (None, _) => false,
        }
    }

    /// Takes the relation of `name` away: where it stood and what it was, to put it back with
    /// [`Relations::restore`]; `None` when there is none.
    pub fn remove(&mut self, kind: Kind, name: &str) -> Option<(usize, Relation)> {
        let at = self.list.iter().position(|r| r.kind == kind && r.name.eq_ignore_ascii_case(name))?;
        Some((at, self.list.remove(at)))
    }

    /// Puts `relation` back at `at`, as [`Relations::remove`] gave it. A relation of the same name set since then
    /// stays and nothing changes; false then.
    pub fn restore(&mut self, at: usize, relation: Relation) -> bool {
        if self.of(relation.kind, &relation.name).is_some() || self.list.len() >= MAX {
            return false;
        }
        self.list.insert(at.min(self.list.len()), relation);
        true
    }

    /// The stance that counts for a handle with its organizations (the main one first), and what it was set on.
    pub fn for_handle<'a>(
        &self,
        handle: &str,
        sids: impl IntoIterator<Item = &'a str>,
    ) -> Option<(Stance, Kind, String)> {
        if let Some(stance) = self.of(Kind::Handle, handle) {
            return Some((stance, Kind::Handle, handle.to_string()));
        }
        sids.into_iter().find_map(|sid| self.of(Kind::Org, sid).map(|stance| (stance, Kind::Org, sid.to_string())))
    }

    fn to_value(&self) -> Value {
        let entries = self
            .list
            .iter()
            .map(|r| {
                Value::Object(vec![
                    ("art".into(), Value::str(r.kind.word())),
                    ("name".into(), Value::str(&r.name)),
                    ("haltung".into(), Value::str(r.stance.word())),
                ])
            })
            .collect();
        Value::Object(vec![
            ("format".into(), Value::str(FORMAT)),
            ("version".into(), Value::int(VERSION)),
            ("eintraege".into(), Value::Array(entries)),
        ])
    }

    fn from_value(data: &Value) -> Result<Relations, ImportError> {
        if data.get("format").and_then(Value::as_str) != Some(FORMAT) {
            return Err(ImportError::WrongForm);
        }
        let Some(Value::Array(entries)) = data.get("eintraege") else { return Err(ImportError::WrongForm) };
        let mut relations = Relations::default();
        for entry in entries {
            let Some(map) = entry.as_object() else { continue };
            let field = |key: &str| map.iter().find(|(k, _)| k == key).and_then(|(_, v)| v.as_str());
            if let (Some(kind), Some(name), Some(stance)) =
                (field("art").and_then(Kind::from_word), field("name"), field("haltung").and_then(Stance::from_word))
            {
                relations.set(kind, name, Some(stance));
            }
        }
        Ok(relations)
    }

    /// The list as a file to hand on.
    pub fn export_text(&self) -> String {
        jsonio::dumps(&self.to_value(), Some(1), false)
    }

    /// Takes a list handed on: its relations are added, a stance it sets replaces the one here. How many relations
    /// changed.
    ///
    /// # Errors
    ///
    /// [`ImportError`] when the text is no list of relations; nothing is changed then.
    pub fn import_text(&mut self, text: &str) -> Result<usize, ImportError> {
        let data = jsonio::loads(text.trim_start_matches('\u{feff}')).map_err(|_| ImportError::NotJson)?;
        let other = Relations::from_value(&data)?;
        Ok(other.list.iter().filter(|r| self.set(r.kind, &r.name, Some(r.stance))).count())
    }

    /// Writes the list to hand on into `path`.
    ///
    /// # Errors
    ///
    /// When the file cannot be written.
    pub fn export_to(&self, path: &str) -> std::io::Result<()> {
        jsonio::write_text_atomic(path, &self.export_text(), jsonio::Newline::Native)
    }

    /// Reads a list handed on from `path` and takes it, see [`Relations::import_text`].
    ///
    /// # Errors
    ///
    /// [`ImportError::NotJson`] when the file cannot be read.
    pub fn import_from(&mut self, path: &str) -> Result<usize, ImportError> {
        let text = jsonio::read_text(path).map_err(|_| ImportError::NotJson)?;
        self.import_text(&text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testdir::Dir;

    fn paths(dir: &Dir) -> Paths {
        let home = dir.path().to_string_lossy().into_owned();
        Paths::new(crate::paths::Host::new(
            crate::pypath::Flavor::native(),
            &[(crate::legacy::ENV_HOME, home.as_str())],
            &home,
        ))
    }

    #[test]
    fn a_stance_is_set_changed_and_taken_away() {
        let mut relations = Relations::default();
        assert!(relations.set(Kind::Handle, "Musterpilot", Some(Stance::Hostile)));
        assert_eq!(relations.of(Kind::Handle, "MUSTERPILOT"), Some(Stance::Hostile));
        assert_eq!(relations.of(Kind::Org, "Musterpilot"), None, "a handle is no organization");
        assert!(!relations.set(Kind::Handle, "Musterpilot", Some(Stance::Hostile)), "nothing changed");
        assert!(relations.set(Kind::Handle, "musterpilot", Some(Stance::Friendly)));
        assert_eq!(relations.list().len(), 1);
        assert!(relations.set(Kind::Handle, "Musterpilot", None));
        assert!(relations.is_empty());
        assert!(!relations.set(Kind::Handle, "Musterpilot", None));
        assert!(!relations.set(Kind::Org, "../böse", Some(Stance::Hostile)), "no SID has these characters");
        assert!(relations.is_empty());
    }

    #[test]
    fn a_relation_removed_comes_back_at_its_place() {
        let mut relations = Relations::default();
        relations.set(Kind::Handle, "Eins", Some(Stance::Hostile));
        relations.set(Kind::Org, "ZWEI", Some(Stance::Friendly));
        relations.set(Kind::Handle, "Drei", Some(Stance::Neutral));
        let before = relations.clone();
        let (at, gone) = relations.remove(Kind::Org, "zwei").expect("there");
        assert_eq!((at, gone.name.as_str(), gone.stance), (1, "ZWEI", Stance::Friendly));
        assert_eq!(relations.of(Kind::Org, "ZWEI"), None);
        assert_eq!(relations.remove(Kind::Org, "ZWEI"), None, "gone already");
        assert_eq!(relations.remove(Kind::Handle, "ZWEI"), None, "an organization is no handle");
        assert!(relations.restore(at, gone.clone()));
        assert_eq!(relations, before, "the same place, the same stance");
        // Counter-check: set anew in the meantime, the newer stance stays.
        let (at, gone) = relations.remove(Kind::Org, "ZWEI").expect("there");
        relations.set(Kind::Org, "ZWEI", Some(Stance::Hostile));
        assert!(!relations.restore(at, gone));
        assert_eq!(relations.of(Kind::Org, "ZWEI"), Some(Stance::Hostile));
        assert_eq!(relations.list().len(), 3);
    }

    #[test]
    fn the_handle_counts_before_its_organizations() {
        let mut relations = Relations::default();
        relations.set(Kind::Org, "TEST", Some(Stance::Friendly));
        relations.set(Kind::Org, "ZWEI", Some(Stance::Hostile));
        assert_eq!(
            relations.for_handle("Musterpilot", ["TEST", "ZWEI"]),
            Some((Stance::Friendly, Kind::Org, "TEST".to_string())),
            "the main organization first"
        );
        assert_eq!(relations.for_handle("Musterpilot", ["DREI", "zwei"]).map(|r| r.0), Some(Stance::Hostile));
        relations.set(Kind::Handle, "Musterpilot", Some(Stance::Neutral));
        assert_eq!(
            relations.for_handle("Musterpilot", ["TEST"]).map(|r| (r.0, r.1)),
            Some((Stance::Neutral, Kind::Handle))
        );
        assert_eq!(relations.for_handle("Andere", ["DREI"]), None);
    }

    #[test]
    fn the_list_is_kept_in_the_data_folder() {
        let dir = Dir::new("beziehungen-datei");
        let paths = paths(&dir);
        assert!(Relations::load(&paths).is_empty());
        let mut relations = Relations::default();
        relations.set(Kind::Handle, "Musterpilot", Some(Stance::Hostile));
        relations.set(Kind::Org, "TEST", Some(Stance::Friendly));
        assert!(relations.save(&paths));
        assert_eq!(Relations::load(&paths), relations);
        let text = std::fs::read_to_string(paths.app_file(FILE)).expect("written");
        assert!(text.contains("\"feindlich\"") && text.contains("\"art\": \"org\""), "{text}");
    }

    #[test]
    fn a_list_handed_on_is_added_and_overrides_the_stance() {
        let mut mine = Relations::default();
        mine.set(Kind::Handle, "Musterpilot", Some(Stance::Friendly));
        mine.set(Kind::Org, "TEST", Some(Stance::Neutral));
        let mut theirs = Relations::default();
        theirs.set(Kind::Handle, "Musterpilot", Some(Stance::Hostile));
        theirs.set(Kind::Handle, "Neuling", Some(Stance::Friendly));
        theirs.set(Kind::Org, "TEST", Some(Stance::Neutral));
        assert_eq!(mine.import_text(&theirs.export_text()), Ok(2));
        assert_eq!(mine.of(Kind::Handle, "Musterpilot"), Some(Stance::Hostile));
        assert_eq!(mine.of(Kind::Handle, "Neuling"), Some(Stance::Friendly));
        assert_eq!(mine.list().len(), 3);
        // The English words of another tool's list are understood, broken entries skipped.
        let english = r#"{"format": "versekit-beziehungen", "eintraege": [
            {"art": "handle", "name": "Englisch", "haltung": "hostile"},
            {"art": "schiff", "name": "X", "haltung": "hostile"},
            {"art": "handle", "name": "a b", "haltung": "hostile"},
            {"art": "handle", "name": "Ohne"}, 7]}"#;
        assert_eq!(mine.import_text(english), Ok(1));
        assert_eq!(mine.of(Kind::Handle, "Englisch"), Some(Stance::Hostile));
    }

    #[test]
    fn a_text_that_is_no_list_changes_nothing() {
        let mut mine = Relations::default();
        mine.set(Kind::Handle, "Musterpilot", Some(Stance::Friendly));
        let before = mine.clone();
        assert_eq!(mine.import_text("kein json"), Err(ImportError::NotJson));
        assert_eq!(mine.import_text(r#"{"format": "anderes", "eintraege": []}"#), Err(ImportError::WrongForm));
        assert_eq!(mine.import_text(r#"{"format": "versekit-beziehungen"}"#), Err(ImportError::WrongForm));
        assert_eq!(mine, before);
        // Counter-check: the right form with a byte order mark is taken.
        assert_eq!(mine.import_text(&format!("\u{feff}{}", Relations::default().export_text())), Ok(0));
    }

    #[test]
    fn export_and_import_go_through_a_file() {
        let dir = Dir::new("beziehungen-weitergabe");
        let file = dir.path().join("liste.json").to_string_lossy().into_owned();
        let mut mine = Relations::default();
        mine.set(Kind::Org, "TEST", Some(Stance::Hostile));
        mine.export_to(&file).expect("written");
        let mut other = Relations::default();
        assert_eq!(other.import_from(&file), Ok(1));
        assert_eq!(other, mine);
        assert_eq!(other.import_from(&format!("{file}.fehlt")), Err(ImportError::NotJson));
        assert!(mine.export_text().contains("\"format\": \"versekit-beziehungen\""));
    }

    #[test]
    fn the_words_of_the_stances_go_both_ways() {
        for stance in Stance::ALL {
            assert_eq!(Stance::from_word(stance.word()), Some(stance));
        }
        assert_eq!(Stance::from_word(" Friendly "), Some(Stance::Friendly));
        assert_eq!(Stance::from_word("unbekannt"), None);
    }
}
