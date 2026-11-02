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

//! The data of the gallery „Was ist neu in X“: shown once after an update, then closed for good.
//!
//! Each series of versions (major.minor) has a handful of highlights with a picture, a title, a short text and
//! the way to the place; after them the other points of the change log of the series as a list. The texts stand
//! in the language files, the pictures under `assets/galerie/<picture>.png` (`-en` for English).
//!
//! When it shows: when the main window opens and the series of the own version has highlights that were not
//! shown yet. A fresh install gets no gallery: whoever missed nothing needs no news.

use crate::changelog::{self, Entry, Kind};
use crate::jsonio::{self, Value};
use crate::language::Lang;
use crate::paths::Paths;
use crate::text::py_strip;
use crate::version::parts;

/// The setting that holds the version whose gallery was closed.
pub const SETTING: &str = "galerie_gesehen";

/// The file of what the user has seen, in the data folder; without `zuletzt` the install is fresh.
pub const SEEN_FILE: &str = "gesehen.json";

/// Where „Ansehen“ leads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    /// A page of the menu, by the key of its name (`hf_grafik`).
    Page(&'static str),
    /// The start page with the cards.
    Overview,
    /// Switches to the view of cards.
    CardView,
}

/// One highlight of a series.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Highlight {
    /// New, better or fixed, as in the change log.
    pub kind: Kind,
    /// The name of the picture under `assets/galerie` (without `.png` and `-en`).
    pub picture: &'static str,
    pub target: Target,
    /// Key of the title.
    pub title: &'static str,
    /// Key of the text.
    pub text: &'static str,
    /// Key of the way to the place.
    pub way: &'static str,
    /// The start of the bold title of the point in the change log (German, English); these points are left out
    /// of the list after the highlights.
    pub changelog: (&'static str, &'static str),
}

/// The highlights of each series, in the order of the gallery; the version names its series.
pub const HIGHLIGHTS: [(&str, &[Highlight]); 3] = [
    (
        "3.97.0",
        &[
            Highlight {
                kind: Kind::New,
                picture: "grafik",
                target: Target::Page("hf_grafik"),
                title: "gal_grafik_t",
                text: "gal_grafik_x_2",
                way: "gal_grafik_w",
                changelog: ("Grafik", "Graphics"),
            },
            Highlight {
                kind: Kind::New,
                picture: "bedarf",
                target: Target::Page("hf_bedarf"),
                title: "gal_bedarf_t",
                text: "gal_bedarf_x_2",
                way: "gal_bedarf_w",
                changelog: ("Bedarf der Einheit", "Unit demand"),
            },
            Highlight {
                kind: Kind::New,
                picture: "lager",
                target: Target::Page("hf_meinlager"),
                title: "gal_raffinerie_t",
                text: "gal_raffinerie_x_2",
                way: "gal_raffinerie_w",
                changelog: ("Raffinerie per Tastenkombination", "Read the refinery with a shortcut"),
            },
            Highlight {
                kind: Kind::Better,
                picture: "shader",
                target: Target::Page("hf_shader"),
                title: "gal_spiel_t",
                text: "gal_spiel_x",
                way: "gal_spiel_w",
                changelog: ("Spiel-Einstellungen in einer eigenen Gruppe", "Game settings in their own group"),
            },
        ],
    ),
    (
        "4.0.0",
        &[
            Highlight {
                kind: Kind::New,
                picture: "karten",
                target: Target::CardView,
                title: "gal_karten_t",
                text: "gal_karten_x_2",
                way: "gal_karten_w",
                changelog: ("Kartenansicht", "Card view"),
            },
            Highlight {
                kind: Kind::New,
                picture: "deine_karten",
                target: Target::Overview,
                title: "gal_deine_t",
                text: "gal_deine_x_2",
                way: "gal_deine_w",
                changelog: ("Deine Karten", "Your cards"),
            },
            Highlight {
                kind: Kind::New,
                picture: "basetool",
                target: Target::Page("k_basetool_verbindung"),
                title: "gal_basetool_t",
                text: "gal_basetool_x",
                way: "gal_basetool_w",
                changelog: ("Basetool als eigene Karte", "Basetool as its own card"),
            },
        ],
    ),
    (
        "5.0.0",
        &[
            Highlight {
                kind: Kind::New,
                picture: "korb",
                target: Target::Page("hf_einkaufsliste"),
                title: "gal5_korb_t",
                text: "gal5_korb_x",
                way: "gal5_korb_w",
                changelog: ("Einkaufskorb aus den Shops", "Shopping basket from the shops"),
            },
            Highlight {
                kind: Kind::New,
                picture: "laut_log",
                target: Target::Page("hf_meinlager"),
                title: "gal5_kauf_t",
                text: "gal5_kauf_x",
                way: "gal5_kauf_w",
                changelog: (
                    "Käufe im Laden landen als Vorschlag in Mein Lager",
                    "Shop purchases show up as suggestions in My Inventory",
                ),
            },
            Highlight {
                kind: Kind::New,
                picture: "zeitraum",
                target: Target::Page("hf_liste"),
                title: "gal5_zeitraum_t",
                text: "gal5_zeitraum_x",
                way: "gal5_zeitraum_w",
                changelog: ("Baupläne nach Zeitraum", "Blueprints by period"),
            },
            Highlight {
                kind: Kind::New,
                picture: "streamer",
                target: Target::Page("hf_allgemein"),
                title: "gal5_streamer_t",
                text: "gal5_streamer_x",
                way: "gal5_streamer_w",
                changelog: ("Streamer-Modus", "Streamer mode"),
            },
            Highlight {
                kind: Kind::New,
                picture: "overlay_ereignisse",
                target: Target::Page("hf_overlay"),
                title: "gal5_overlay_t",
                text: "gal5_overlay_x",
                way: "gal5_overlay_w",
                changelog: ("Das Overlay klappt bei den Ereignissen auf", "The overlay opens on the events you choose"),
            },
        ],
    ),
];

/// (major, minor): patch versions belong to the gallery of their series.
fn series(version: &str) -> (u64, u64) {
    let (major, minor, _) = parts(version);
    (major, minor)
}

/// The highlights of the series of this version (3.97.1 gives those of 3.97.0), or none.
pub fn highlights(version: &str) -> &'static [Highlight] {
    HIGHLIGHTS.iter().find(|(key, _)| series(key) == series(version)).map_or(&[], |(_, points)| *points)
}

/// Whether a value of the seen file counts as set: not missing, not empty, not false, not zero.
fn is_set(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null) | Some(Value::Bool(false)) => false,
        Some(Value::Str(text)) => !text.is_empty(),
        Some(Value::Int(text)) => text != "0",
        Some(Value::Float(number)) => *number != 0.0,
        Some(Value::Array(items)) => !items.is_empty(),
        Some(Value::Object(map)) => !map.is_empty(),
        Some(Value::Bool(true)) => true,
    }
}

/// The version run before as the seen file in the data folder knows it (`zuletzt`), if it knows one.
fn last_run(paths: &Paths) -> Option<String> {
    match jsonio::read_json(&paths.app_file(SEEN_FILE)) {
        Ok(data @ Value::Object(_)) if is_set(data.get("zuletzt")) => data.get("zuletzt").map(crate::report::py_text),
        _ => None,
    }
}

/// Whether the gallery is to show now. Asking writes nothing.
///
/// Once for each series: whoever closed it under 3.97.0 does not get it again under 3.97.1. Without the setting
/// of a closed gallery the version run before decides: one of an older series is an update and brings the
/// gallery; a fresh install (no version run before) shows nothing, and from its first start on the seen file
/// holds a version of the own series. ⚠ Asked before anything writes the seen file in this start, or an update
/// would count as a fresh install.
pub fn due(paths: &Paths, version: &str) -> bool {
    if highlights(version).is_empty() {
        return false;
    }
    if let Some(seen) = paths.setting(SETTING) {
        return series(&seen) < series(version);
    }
    last_run(paths).is_some_and(|last| series(&last) < series(version))
}

/// The gallery of this version was closed. Returns whether the setting was written.
pub fn mark_shown(paths: &Paths, version: &str) -> bool {
    let (major, minor, patch) = parts(version);
    paths.set_setting(SETTING, Value::str(format!("{major}.{minor}.{patch}")))
}

/// The bold title of a point of the change log, else the whole line.
fn title(line: &str) -> String {
    let stripped = line.trim_start_matches(crate::text::py_isspace);
    if let Some(rest) = stripped.strip_prefix("**") {
        // The title has at least one sign: the closing marks are looked for after the first one.
        let first = rest.chars().next().map_or(0, char::len_utf8);
        if first > 0
            && let Some(end) = rest[first..].find("**")
        {
            return py_strip(&rest[..first + end]).to_string();
        }
    }
    py_strip(line).to_string()
}

/// The other points of the series: `(kind, title)` without the highlights.
///
/// Read are all entries of `entries` of the series up to the own version (3.97.0 and 3.97.1 under 3.97.1), older
/// first; a point that stands twice comes once. `entries` is the history in `lang`
/// ([`changelog::history`]).
pub fn rest(version: &str, lang: Lang, entries: &[Entry]) -> Vec<(Kind, String)> {
    let own = parts(version);
    let mut chosen: Vec<&Entry> =
        entries.iter().filter(|e| series(&e.version) == series(version) && parts(&e.version) <= own).collect();
    chosen.sort_by_key(|e| parts(&e.version));
    let skip: Vec<&str> =
        highlights(version).iter().map(|h| if lang == Lang::En { h.changelog.1 } else { h.changelog.0 }).collect();
    let mut out: Vec<(Kind, String)> = Vec::new();
    for entry in chosen {
        for (kind, line) in changelog::points_by_kind(&entry.text) {
            let title = title(&line);
            if skip.iter().any(|s| title.starts_with(s)) || out.iter().any(|(k, t)| *k == kind && *t == title) {
                continue;
            }
            out.push((kind, title));
        }
    }
    out
}

/// [`rest`] on the change log built into the program, in `lang`.
pub fn rest_built_in(version: &str, lang: Lang) -> Vec<(Kind, String)> {
    rest(version, lang, &changelog::history(lang, &[]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::Host;
    use crate::pypath::Flavor;
    use crate::testdir::Dir;

    fn paths_in(name: &str) -> (Dir, Paths) {
        let folder = Dir::new(name);
        let text = folder.path().to_string_lossy().into_owned();
        let host = Host::new(Flavor::Posix, &[("SC_BP_HOME", &text)], &text);
        (folder, Paths::new(host))
    }

    fn write_seen(paths: &Paths, text: &str) {
        std::fs::write(paths.app_file(SEEN_FILE), text).expect("seen file");
    }

    fn entry(version: &str, text: &str) -> Entry {
        Entry { version: version.to_string(), date: String::new(), text: text.to_string(), built_in: true }
    }

    #[test]
    fn a_patch_version_gets_the_highlights_of_its_series() {
        assert_eq!(highlights("4.0.1"), highlights("4.0.0"));
        assert_eq!(highlights("v4.0.3-rc2").len(), 3);
        assert_eq!(highlights("3.97.2")[0].picture, "grafik");
        assert_eq!(highlights("3.97.0").len(), 4);
        assert!(highlights("4.1.0").is_empty());
        assert_eq!(highlights("5.0.0-dev"), highlights("5.0.0"), "a test build of 5.0 shows the gallery of 5.0");
        assert_eq!(highlights("5.0.0").len(), 5);
        assert!(highlights("5.1.0").is_empty());
        assert!(highlights("").is_empty());
    }

    /// Each highlight of 5.0 is a point of the change log of 5.0 in both languages, under its kind, and all its
    /// texts and the name of its page stand in both language files.
    #[test]
    fn every_highlight_of_5_0_is_a_point_of_its_change_log_with_its_texts() {
        for lang in Lang::ALL {
            let entries = changelog::history(lang, &[]);
            let own = entries.iter().find(|e| parts(&e.version) == (5, 0, 0)).expect("the block of 5.0.0");
            let points: Vec<(Kind, String)> =
                changelog::points_by_kind(&own.text).into_iter().map(|(kind, line)| (kind, title(&line))).collect();
            for point in highlights("5.0.0") {
                let start = if lang == Lang::En { point.changelog.1 } else { point.changelog.0 };
                assert!(
                    points.iter().any(|(kind, title)| *kind == point.kind && title.starts_with(start)),
                    "{lang:?}: {start} not among {points:?}"
                );
                let Target::Page(page) = point.target else { panic!("{start}: a page of the menu") };
                for key in [point.title, point.text, point.way, page] {
                    assert!(crate::language::text_in(lang, key).is_some(), "{lang:?}: no text {key}");
                }
            }
        }
    }

    #[test]
    fn every_highlight_has_its_picture_in_both_languages() {
        let folder = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/galerie");
        for (_, points) in HIGHLIGHTS {
            for point in points {
                for name in [format!("{}.png", point.picture), format!("{}-en.png", point.picture)] {
                    assert!(folder.join(&name).is_file(), "{name}");
                }
            }
        }
    }

    #[test]
    fn the_gallery_comes_once_for_each_series() {
        let (_folder, paths) = paths_in("gallery-once");
        write_seen(&paths, r#"{"zuletzt": "3.96.0", "bereiche": {}}"#);
        assert!(due(&paths, "4.0.0"), "an update into a series with highlights");
        assert!(due(&paths, "4.0.0"), "asking again changes nothing");
        assert!(mark_shown(&paths, "4.0.0"));
        assert_eq!(paths.setting(SETTING).as_deref(), Some("4.0.0"));
        assert!(!due(&paths, "4.0.0"), "closed");
        assert!(!due(&paths, "4.0.1"), "a patch of the same series");
        assert!(!due(&paths, "4.1.0"), "a series without highlights");
        // Closed under an older series: the newer one shows again.
        assert!(mark_shown(&paths, "v3.97.2"));
        assert_eq!(paths.setting(SETTING).as_deref(), Some("3.97.2"));
        assert!(due(&paths, "4.0.2"));
    }

    #[test]
    fn a_fresh_install_shows_nothing_and_writes_nothing() {
        // No seen file at all.
        let (_folder, paths) = paths_in("gallery-fresh");
        assert!(!due(&paths, "5.0.0-dev"));
        assert!(!std::path::Path::new(&paths.app_file("einstellungen.json")).exists(), "asking writes nothing");
        // A seen file without `zuletzt`, and one where it is empty: fresh as well.
        for (name, text) in
            [("gallery-fresh-areas", r#"{"bereiche": {}}"#), ("gallery-fresh-empty", r#"{"zuletzt": ""}"#)]
        {
            let (_folder, paths) = paths_in(name);
            write_seen(&paths, text);
            assert!(!due(&paths, "3.97.0"), "{text}");
            assert_eq!(paths.setting(SETTING), None, "{text}");
        }
        // From the first start on the seen file holds the own series: the next start stays quiet too.
        write_seen(&paths, r#"{"zuletzt": "5.0.0-dev", "bereiche": {}}"#);
        assert!(!due(&paths, "5.0.0-dev"));
        assert!(!due(&paths, "5.0.1"), "a patch of the series is no update into it");
        // Coming from 4.12 is an update into 5.0.
        write_seen(&paths, r#"{"zuletzt": "4.12.3", "bereiche": {}}"#);
        assert!(due(&paths, "5.0.0-dev"));
    }

    #[test]
    fn the_rest_leaves_out_the_highlights_and_takes_the_patches_of_the_series() {
        let entries = [
            entry("v4.1.0", "### Neu\n- **Später** gehört nicht dazu"),
            entry(
                "v4.0.1",
                "### Behoben\n- **Fehler A** behoben\n- **Kartenansicht** klappt wieder\n### Neu\n- **Spät dazu**",
            ),
            entry(
                "v4.0.0",
                "> Vorspann\n\n### Neu\n- **Kartenansicht** — jeder Bereich\n- **Deine Karten** — Nadel\n\
                 - **Basetool als eigene Karte** — alles\n- **Suche** — schneller\n### Verbessert\n- Ohne Titel\n\
                 ### Behoben\n- **Fehler A** behoben",
            ),
            entry("v3.99.0", "### Neu\n- **Vorher** gehört nicht dazu"),
        ];
        let rest = rest("4.0.1", Lang::De, &entries);
        assert_eq!(
            rest,
            vec![
                (Kind::New, "Suche".to_string()),
                (Kind::Better, "Ohne Titel".to_string()),
                (Kind::Fix, "Fehler A".to_string()),
                (Kind::New, "Spät dazu".to_string()),
            ]
        );
        // Under 4.0.0 the patch after it is not read yet.
        let older = super::rest("4.0.0", Lang::De, &entries);
        assert!(!older.iter().any(|(_, t)| t == "Spät dazu"), "{older:?}");
        // English titles are matched by the English start.
        let english = [entry("v4.0.0", "### Added\n- **Card view** — every area\n- **Search** — faster")];
        assert_eq!(super::rest("4.0.0", Lang::En, &english), vec![(Kind::New, "Search".to_string())]);
    }

    #[test]
    fn the_rest_of_the_built_in_log_has_no_highlight() {
        for lang in Lang::ALL {
            for (version, points) in HIGHLIGHTS {
                let rest = rest_built_in(version, lang);
                assert!(!rest.is_empty(), "{version} {lang:?}");
                for point in points {
                    let start = if lang == Lang::En { point.changelog.1 } else { point.changelog.0 };
                    assert!(!rest.iter().any(|(_, t)| t.starts_with(start)), "{version} {lang:?}: {start}");
                }
            }
        }
    }

    #[test]
    fn a_title_is_the_bold_start_of_the_point() {
        assert_eq!(title("**Kartenansicht** — jeder Bereich"), "Kartenansicht");
        assert_eq!(title("  **Fett**"), "Fett");
        assert_eq!(title("Ohne Titel "), "Ohne Titel");
        assert_eq!(title("****leer"), "****leer");
        assert_eq!(title("***a** b"), "*a");
    }
}
