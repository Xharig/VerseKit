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

//! Own names for the languages of the translation list: the player may call "Deutsch" what suits them
//! ("Deutsch (Crew)", "DE für den Stream"). Only the name on the page changes; the key of the language (its folder
//! in the game) and what is written into the game stay as they are.
//!
//! The names live in the settings as an object `{key of the language: name}` under [`SETTING`].

use crate::jsonio::Value;
use crate::paths::Paths;
use crate::text::py_strip;

/// The setting that holds the own names: `{key of the language: name}`.
pub const SETTING: &str = "sprachnamen";

/// The longest own name, in characters. A longer one is cut: the button must not outgrow the window.
pub const LONGEST: usize = 40;

/// The name chosen for a language, `None` where there is none.
pub fn custom(paths: &Paths, key: &str) -> Option<String> {
    let Some((_, Value::Object(names))) = paths.settings().into_iter().find(|(k, _)| k == SETTING) else {
        return None;
    };
    names.into_iter().find(|(k, _)| k == key).and_then(|(_, v)| match v {
        Value::Str(name) => Some(name).filter(|n| !n.is_empty()),
        _ => None,
    })
}

/// A name as it is kept: blanks around it cut, control characters out, no more than [`LONGEST`] characters.
/// Empty where nothing is left.
pub fn clean(text: &str) -> String {
    let text: String = text.chars().filter(|c| !c.is_control()).collect();
    py_strip(&text).chars().take(LONGEST).collect::<String>().trim_end().to_string()
}

/// Gives the language `key` the name `name`; an empty name (after [`clean`]) takes the own name away again.
/// Gives whether the setting was written.
pub fn set(paths: &Paths, key: &str, name: &str) -> bool {
    let name = clean(name);
    let mut names = match paths.settings().into_iter().find(|(k, _)| k == SETTING).map(|(_, v)| v) {
        Some(Value::Object(names)) => names,
        _ => Vec::new(),
    };
    names.retain(|(k, _)| k != key);
    if !name.is_empty() {
        names.push((key.to_string(), Value::Str(name)));
    }
    paths.set_setting(SETTING, Value::Object(names))
}

/// The name to show for the language `key`: the own one, else `default`.
pub fn shown(paths: &Paths, key: &str, default: &str) -> String {
    custom(paths, key).unwrap_or_else(|| default.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::Host;
    use crate::pypath::Flavor;

    fn paths_in(name: &str) -> (Paths, String) {
        let dir = std::env::temp_dir().join(format!("versekit-langnames-{}-{name}", crate::testdir::run_tag()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("folder");
        let home = dir.to_string_lossy().replace('\\', "/");
        (Paths::new(Host::new(Flavor::Posix, &[("VERSEKIT_HOME", &home), ("HOME", &home)], &home)), home)
    }

    #[test]
    fn an_own_name_is_kept_per_language_replaced_and_taken_away_again() {
        let (paths, home) = paths_in("kept");
        assert_eq!(shown(&paths, "german_(germany)", "Deutsch"), "Deutsch", "no own name: the usual one");
        assert!(set(&paths, "german_(germany)", "  Deutsch (Crew) "));
        assert!(set(&paths, "luftwerft_bar", "Bairisch"));
        assert_eq!(shown(&paths, "german_(germany)", "Deutsch"), "Deutsch (Crew)");
        assert_eq!(shown(&paths, "luftwerft_bar", "Bayrisch"), "Bairisch", "another language, its own name");
        assert!(set(&paths, "german_(germany)", "Deutsch (Stream)"));
        assert_eq!(custom(&paths, "german_(germany)").as_deref(), Some("Deutsch (Stream)"));
        // Read again from the file: it is kept, not only remembered.
        paths.forget_settings();
        assert_eq!(custom(&paths, "german_(germany)").as_deref(), Some("Deutsch (Stream)"));
        assert!(set(&paths, "german_(germany)", "   "));
        assert_eq!(shown(&paths, "german_(germany)", "Deutsch"), "Deutsch", "empty: back to the usual name");
        assert_eq!(custom(&paths, "luftwerft_bar").as_deref(), Some("Bairisch"), "the other one stays");
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn a_name_is_cleaned_before_it_is_kept() {
        assert_eq!(clean("  Deutsch\n\t"), "Deutsch");
        assert_eq!(clean(""), "");
        let long = "x".repeat(LONGEST + 20);
        assert_eq!(clean(&long).chars().count(), LONGEST);
        assert_eq!(clean("a\u{7}b"), "ab", "no control characters");
    }
}
